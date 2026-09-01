//! Canonical one-pass RecordBatch XLSX writing.

use std::collections::{BTreeMap, BTreeSet};

use arrow::array::Array as ArrowArray;
use arrow::record_batch::RecordBatchT;
use rust_xlsxwriter::{Format, Workbook};

use crate::constant::{NCOLS_SHEET_MAX, NROWS_SHEET_MAX};
use crate::spec::{
    AutofitMode, AutofitPolicy, ScientificPolicy, SheetSlice, WarningCode, XlsxReport,
    XlsxValuePolicy,
};
use crate::util::{
    calculate_row_chunk_size, plan_sheet_slices, sanitize_sheet_name,
    select_sorted_indices_from_refs, validate_unique_columns,
};

#[cfg(test)]
use super::TestFinalizeFailurePoint;
use super::render::{
    ColumnFormatPlanOptions, apply_column_widths, cast_col_num, cast_row_num,
    create_rust_xlsx_format, format_xlsx_error_text, inferred_num_formats, plan_column_formats,
    plan_header_formats, plan_scientific_formats, slice_column_format_overrides,
    write_cell_with_format, write_header,
};
use super::value::{
    NormalizedCell, normalize_arrow_cell, select_integer_column_indices_from_arrow_schema,
    select_numeric_column_indices_from_arrow_schema, validate_policy_autofit,
    validate_policy_scientific,
};
use super::{XlsxSheetWriteOptions, XlsxWriter};

/// Arrow record batch shape accepted by the streaming writer.
pub type XlsxRecordBatch = RecordBatchT<Box<dyn ArrowArray>>;
/// Fallible Arrow record batch item accepted by bridge streaming sessions.
pub type XlsxRecordBatchResult = Result<XlsxRecordBatch, String>;

struct XlsxSheetRuntime {
    worksheet_index: usize,
    sheet_slice: SheetSlice,
    data_formats_by_col: Vec<Format>,
    scientific_formats_by_col: Vec<Format>,
    numeric_cols_idx: BTreeSet<usize>,
    integer_cols_idx: BTreeSet<usize>,
    decimal_cols_idx: BTreeSet<usize>,
    is_decimal_explicit: bool,
}

struct XlsxSinglePassPlan {
    col_names: Vec<String>,
    header_grid: Vec<Vec<String>>,
    cols_idx_numeric: Vec<usize>,
    cols_idx_integer: Vec<usize>,
    cols_idx_decimal_specified: Vec<usize>,
    num_frozen_rows: usize,
    should_keep_missing_values: bool,
}

#[derive(Debug)]
struct LogicalAutofitTracker {
    mode: AutofitMode,
    max_rows: Option<usize>,
    header_widths_by_col: Vec<usize>,
    body_widths_by_col: Vec<usize>,
}

impl LogicalAutofitTracker {
    fn new(
        width: usize,
        header_grid: &[Vec<String>],
        policy: &AutofitPolicy,
        should_keep_missing_values: bool,
        value_policy: &XlsxValuePolicy,
    ) -> Self {
        let mut tracker = Self {
            mode: policy.mode,
            max_rows: policy.height_body_inferred_max,
            header_widths_by_col: vec![0; width],
            body_widths_by_col: vec![0; width],
        };
        if matches!(tracker.mode, AutofitMode::Header | AutofitMode::All) {
            for row in header_grid {
                for (col_abs, value) in row.iter().enumerate() {
                    let normalized = NormalizedCell::from_header(value.clone());
                    tracker.header_widths_by_col[col_abs] = tracker.header_widths_by_col[col_abs]
                        .max(normalized.estimated_width(should_keep_missing_values, value_policy));
                }
            }
        }
        tracker
    }

    fn observe(
        &mut self,
        row_abs: usize,
        col_abs: usize,
        cell: &NormalizedCell,
        should_keep_missing_values: bool,
        value_policy: &XlsxValuePolicy,
    ) {
        if !matches!(self.mode, AutofitMode::Body | AutofitMode::All)
            || self.max_rows.is_some_and(|max_rows| row_abs >= max_rows)
        {
            return;
        }
        self.body_widths_by_col[col_abs] = self.body_widths_by_col[col_abs]
            .max(cell.estimated_width(should_keep_missing_values, value_policy));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct PhysicalSheetKey {
    row_start: usize,
    col_start: usize,
    col_end: usize,
}

impl PhysicalSheetKey {
    fn from_slice(sheet: &SheetSlice) -> Self {
        Self {
            row_start: sheet.row_start_inclusive,
            col_start: sheet.col_start_inclusive,
            col_end: sheet.col_end_exclusive,
        }
    }
}

#[derive(Debug)]
struct PhysicalSheetEntry {
    worksheet_index: usize,
    temporary_name: String,
    actual_row_end: usize,
}

#[derive(Default, Debug)]
struct PhysicalSheetRegistry {
    entries: BTreeMap<PhysicalSheetKey, PhysicalSheetEntry>,
}

impl PhysicalSheetRegistry {
    fn register(&mut self, key: PhysicalSheetKey, entry: PhysicalSheetEntry) -> Result<(), String> {
        if self.entries.insert(key, entry).is_some() {
            return Err(format!(
                "Duplicate physical worksheet registry key: {key:?}."
            ));
        }
        Ok(())
    }

    fn observe_row_end(&mut self, key: PhysicalSheetKey, row_end: usize) -> Result<(), String> {
        let entry = self
            .entries
            .get_mut(&key)
            .ok_or_else(|| format!("Missing physical worksheet registry key: {key:?}."))?;
        entry.actual_row_end = entry.actual_row_end.max(row_end);
        Ok(())
    }

    fn into_canonical(mut self, planned: &[SheetSlice]) -> Result<Vec<PhysicalSheetEntry>, String> {
        let mut entries = Vec::with_capacity(planned.len());
        for sheet in planned {
            let key = PhysicalSheetKey::from_slice(sheet);
            let entry = self
                .entries
                .remove(&key)
                .ok_or_else(|| format!("Missing physical worksheet registry key: {key:?}."))?;
            if entry.actual_row_end != sheet.row_end_exclusive {
                return Err(format!(
                    "Physical worksheet row boundary mismatch for {key:?}: planned {} rows but observed {}.",
                    sheet.row_end_exclusive, entry.actual_row_end
                ));
            }
            entries.push(entry);
        }
        if !self.entries.is_empty() {
            return Err(format!(
                "Orphan physical worksheet registry keys: {:?}.",
                self.entries.keys().collect::<Vec<_>>()
            ));
        }
        Ok(entries)
    }
}

struct XlsxSinglePassRuntimeSheet {
    runtime: XlsxSheetRuntime,
    key: PhysicalSheetKey,
}

impl XlsxWriter {
    /// Write one logical sheet from a single fallible RecordBatch stream.
    pub fn write_sheet_from_record_batch_results<I>(
        &mut self,
        batches: I,
        sheet_name: &str,
        header_grid: Option<Vec<Vec<String>>>,
        options: &XlsxSheetWriteOptions,
    ) -> Result<(), String>
    where
        I: IntoIterator<Item = XlsxRecordBatchResult>,
    {
        self.require_open()?;
        options.validate_preflight()?;
        validate_policy_autofit(&options.policy_autofit)?;
        validate_policy_scientific(&options.policy_scientific)?;

        let mut iter_batches = batches.into_iter();
        let Some(first_batch_result) = iter_batches.next() else {
            return Err(
                "Cannot write sheet from an empty batch stream with unknown schema.".to_string(),
            );
        };
        let first_batch = first_batch_result?;
        let plan = self.create_single_pass_plan(&first_batch, header_grid, options)?;
        if plan.col_names.is_empty() {
            return Err("Cannot write a sheet with zero columns.".to_string());
        }

        let worksheet_count_before = self.workbook.worksheets().len();
        let result = self.write_prepared_record_batches(
            first_batch,
            iter_batches,
            sheet_name,
            plan,
            options,
            worksheet_count_before,
        );
        if result.is_err() && self.workbook.worksheets().len() > worksheet_count_before {
            self.mark_poisoned();
        }
        result
    }

    fn write_prepared_record_batches<I>(
        &mut self,
        first_batch: XlsxRecordBatch,
        iter_batches: I,
        sheet_name: &str,
        plan: XlsxSinglePassPlan,
        options: &XlsxSheetWriteOptions,
        worksheet_segment_start: usize,
    ) -> Result<(), String>
    where
        I: Iterator<Item = XlsxRecordBatchResult>,
    {
        let col_names_ref = plan
            .col_names
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let header_row_count = plan.header_grid.len();
        let max_data_rows = NROWS_SHEET_MAX
            .checked_sub(header_row_count)
            .ok_or_else(|| {
                format!("Header too tall: height_header={header_row_count} exceeds Excel limit.")
            })?;
        if max_data_rows == 0 {
            return Err(format!(
                "Header too tall: height_header={header_row_count} exceeds Excel limit."
            ));
        }

        let mut report = XlsxReport {
            sheets: vec![],
            warnings: vec![],
        };
        let mut tracker = LogicalAutofitTracker::new(
            plan.col_names.len(),
            &plan.header_grid,
            &options.policy_autofit,
            plan.should_keep_missing_values,
            &self.options_write.value_policy,
        );
        let mut registry = PhysicalSheetRegistry::default();
        let mut runtime_sheets: Vec<XlsxSinglePassRuntimeSheet> = vec![];
        let mut active_row_start: Option<usize> = None;
        let mut occupied_temporary_names = BTreeSet::new();
        let mut next_temporary_index = 1usize;
        let mut rows_written = 0usize;

        self.write_single_pass_batch(
            &plan,
            options,
            &first_batch,
            &col_names_ref,
            rows_written,
            max_data_rows,
            &mut active_row_start,
            &mut next_temporary_index,
            &mut occupied_temporary_names,
            &mut runtime_sheets,
            &mut registry,
            &mut tracker,
            &mut report,
        )?;
        rows_written += first_batch.len();

        for batch in iter_batches {
            let batch = batch?;
            self.write_single_pass_batch(
                &plan,
                options,
                &batch,
                &col_names_ref,
                rows_written,
                max_data_rows,
                &mut active_row_start,
                &mut next_temporary_index,
                &mut occupied_temporary_names,
                &mut runtime_sheets,
                &mut registry,
                &mut tracker,
                &mut report,
            )?;
            rows_written += batch.len();
        }

        if rows_written == 0 {
            self.ensure_single_pass_runtime_sheets(
                &plan,
                options,
                0,
                max_data_rows,
                &mut active_row_start,
                &mut next_temporary_index,
                &mut occupied_temporary_names,
                &mut runtime_sheets,
                &mut registry,
            )?;
        }

        self.finalize_single_pass_sheet(
            sheet_name,
            rows_written,
            &plan,
            options,
            &tracker,
            registry,
            worksheet_segment_start,
            &mut report,
        )?;
        sort_conversion_warnings(&mut report);
        self.reports.push(report);
        Ok(())
    }

    fn create_single_pass_plan(
        &self,
        first_batch: &XlsxRecordBatch,
        header_grid_custom: Option<Vec<Vec<String>>>,
        options: &XlsxSheetWriteOptions,
    ) -> Result<XlsxSinglePassPlan, String> {
        let schema = first_batch.schema();
        let col_names = schema
            .iter_names()
            .map(|name| name.to_string())
            .collect::<Vec<_>>();
        let col_names_ref = col_names.iter().map(String::as_str).collect::<Vec<_>>();
        validate_unique_columns(&col_names_ref)?;
        let width_body = col_names.len();

        let header_grid = match header_grid_custom {
            Some(header_grid) => {
                if header_grid.is_empty() {
                    return Err(
                        "header must have >= 1 row (0-row header is not allowed).".to_string()
                    );
                }
                if header_grid.iter().any(|row| row.len() != width_body) {
                    return Err("header.width must equal body.width.".to_string());
                }
                header_grid
            }
            None => vec![col_names.clone()],
        };

        let mut cols_idx_numeric = if self.options_write.should_infer_numeric_cols {
            select_numeric_column_indices_from_arrow_schema(schema)
        } else {
            vec![]
        };
        let cols_idx_integer_specified =
            select_sorted_indices_from_refs(&col_names_ref, options.cols_integer.as_deref())?;
        let cols_idx_decimal_specified =
            select_sorted_indices_from_refs(&col_names_ref, options.cols_decimal.as_deref())?;
        cols_idx_numeric.extend(cols_idx_integer_specified.iter().copied());
        cols_idx_numeric.extend(cols_idx_decimal_specified.iter().copied());
        cols_idx_numeric.sort_unstable();
        cols_idx_numeric.dedup();
        let mut cols_idx_integer = if self.options_write.should_infer_integer_cols {
            select_integer_column_indices_from_arrow_schema(schema, &cols_idx_numeric)
        } else {
            vec![]
        };
        cols_idx_integer.retain(|idx| !cols_idx_decimal_specified.contains(idx));
        cols_idx_integer.extend(cols_idx_integer_specified);
        cols_idx_integer.sort_unstable();
        cols_idx_integer.dedup();

        let rows_chunk = calculate_row_chunk_size(width_body, &self.options_write.row_chunk_policy);
        if rows_chunk == 0 {
            return Err("row_chunk_policy resolved to 0 rows; expected >= 1.".to_string());
        }

        let should_keep_missing_values = options
            .should_keep_missing_values
            .unwrap_or(self.options_write.should_keep_missing_values);
        let header_row_count = header_grid.len();
        Ok(XlsxSinglePassPlan {
            col_names,
            header_grid,
            cols_idx_numeric,
            cols_idx_integer,
            cols_idx_decimal_specified,
            num_frozen_rows: options.num_frozen_rows.unwrap_or(header_row_count),
            should_keep_missing_values,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn write_single_pass_batch(
        &mut self,
        plan: &XlsxSinglePassPlan,
        options: &XlsxSheetWriteOptions,
        batch: &XlsxRecordBatch,
        col_names_ref: &[&str],
        row_offset: usize,
        max_data_rows: usize,
        active_row_start: &mut Option<usize>,
        next_temporary_index: &mut usize,
        occupied_temporary_names: &mut BTreeSet<String>,
        runtime_sheets: &mut Vec<XlsxSinglePassRuntimeSheet>,
        registry: &mut PhysicalSheetRegistry,
        tracker: &mut LogicalAutofitTracker,
        report: &mut XlsxReport,
    ) -> Result<(), String> {
        let batch_col_names = batch
            .schema()
            .iter_names()
            .map(|name| name.as_str())
            .collect::<Vec<_>>();
        if batch_col_names != col_names_ref {
            return Err("All record batches must have identical column names.".to_string());
        }

        let batch_start = row_offset;
        let batch_end = row_offset + batch.len();
        let mut segment_start = batch_start;
        while segment_start < batch_end {
            let row_part_start = (segment_start / max_data_rows) * max_data_rows;
            self.ensure_single_pass_runtime_sheets(
                plan,
                options,
                row_part_start,
                max_data_rows,
                active_row_start,
                next_temporary_index,
                occupied_temporary_names,
                runtime_sheets,
                registry,
            )?;

            for runtime in runtime_sheets.iter() {
                write_arrow_record_batch_to_runtime_sheet(
                    &mut self.workbook,
                    &runtime.runtime,
                    batch,
                    row_offset,
                    plan.header_grid.len(),
                    plan.should_keep_missing_values,
                    &self.options_write.value_policy,
                    &options.policy_scientific,
                    &options.value_plans,
                    tracker,
                    report,
                )?;
                let overlap_end = batch_end.min(runtime.runtime.sheet_slice.row_end_exclusive);
                if overlap_end > runtime.runtime.sheet_slice.row_start_inclusive {
                    registry.observe_row_end(runtime.key, overlap_end)?;
                }
            }

            segment_start = batch_end.min(row_part_start + max_data_rows);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn ensure_single_pass_runtime_sheets(
        &mut self,
        plan: &XlsxSinglePassPlan,
        options: &XlsxSheetWriteOptions,
        row_part_start: usize,
        max_data_rows: usize,
        active_row_start: &mut Option<usize>,
        next_temporary_index: &mut usize,
        occupied_temporary_names: &mut BTreeSet<String>,
        runtime_sheets: &mut Vec<XlsxSinglePassRuntimeSheet>,
        registry: &mut PhysicalSheetRegistry,
    ) -> Result<(), String> {
        if active_row_start.is_some_and(|value| value == row_part_start) {
            return Ok(());
        }

        runtime_sheets.clear();
        *active_row_start = Some(row_part_start);

        let width_body = plan.col_names.len();
        let mut col_start = 0usize;
        while col_start < width_body {
            let col_end = width_body.min(col_start + NCOLS_SHEET_MAX);
            let temporary_name = allocate_internal_sheet_name(
                "tmp",
                next_temporary_index,
                occupied_temporary_names,
                &self.existing_sheet_names,
            );
            occupied_temporary_names.insert(temporary_name.clone());

            let worksheet_index = self.workbook.worksheets().len();
            let worksheet = self.workbook.add_worksheet_with_constant_memory();
            worksheet
                .set_name(&temporary_name)
                .map_err(format_xlsx_error_text)?;

            let cols_idx_numeric_slice =
                calculate_slice_indices(&plan.cols_idx_numeric, col_start, col_end);
            let cols_idx_integer_slice =
                calculate_slice_indices(&plan.cols_idx_integer, col_start, col_end);
            let cols_idx_decimal_slice =
                calculate_slice_indices(&plan.cols_idx_decimal_specified, col_start, col_end);
            let inferred_num_formats_all = inferred_num_formats(&options.value_plans);
            let inferred_num_formats_slice = if inferred_num_formats_all.len() >= col_end {
                inferred_num_formats_all[col_start..col_end].to_vec()
            } else {
                vec![None; col_end - col_start]
            };
            let column_formats_slice =
                slice_column_format_overrides(&options.column_formats, col_start, col_end);
            let header_column_formats_slice =
                slice_column_format_overrides(&options.header_column_formats, col_start, col_end);
            let column_format_plan = plan_column_formats(ColumnFormatPlanOptions {
                width_data: col_end - col_start,
                cols_idx_numeric: &cols_idx_numeric_slice,
                cols_idx_integer: &cols_idx_integer_slice,
                cols_idx_decimal: if cols_idx_decimal_slice.is_empty() {
                    None
                } else {
                    Some(&cols_idx_decimal_slice)
                },
                cols_fmt_overrides: &column_formats_slice,
                fmt_text: &self.fmt_text,
                fmt_integer: &self.fmt_integer,
                fmt_decimal: &self.fmt_decimal,
                fmt_text_override: &self.fmt_text_override,
                fmt_integer_override: &self.fmt_integer_override,
                fmt_decimal_override: &self.fmt_decimal_override,
                inferred_num_formats: Some(&inferred_num_formats_slice),
            });
            let data_formats_by_col = column_format_plan
                .fmts_by_col
                .iter()
                .map(create_rust_xlsx_format)
                .collect::<Vec<_>>();
            let scientific_formats_by_col = plan_scientific_formats(
                column_format_plan.fmts_by_col.len(),
                &self.fmt_scientific,
                &column_formats_slice,
            )
            .iter()
            .map(create_rust_xlsx_format)
            .collect::<Vec<_>>();
            let fmt_headers = plan_header_formats(
                &self.fmt_header,
                &options.header_row_formats,
                &header_column_formats_slice,
                plan.header_grid.len(),
                col_end - col_start,
            )?;
            let header_grid_slice = plan
                .header_grid
                .iter()
                .map(|row| row[col_start..col_end].to_vec())
                .collect::<Vec<_>>();
            write_header(
                worksheet,
                header_grid_slice,
                options.should_merge_header,
                &fmt_headers,
            )?;
            worksheet
                .set_freeze_panes(
                    cast_row_num(plan.num_frozen_rows)?,
                    cast_col_num(options.num_frozen_cols)?,
                )
                .map_err(format_xlsx_error_text)?;

            let key = PhysicalSheetKey {
                row_start: row_part_start,
                col_start,
                col_end,
            };
            registry.register(
                key,
                PhysicalSheetEntry {
                    worksheet_index,
                    temporary_name: temporary_name.clone(),
                    actual_row_end: row_part_start,
                },
            )?;
            runtime_sheets.push(XlsxSinglePassRuntimeSheet {
                runtime: XlsxSheetRuntime {
                    worksheet_index,
                    sheet_slice: SheetSlice {
                        sheet_name: temporary_name,
                        row_start_inclusive: row_part_start,
                        row_end_exclusive: row_part_start + max_data_rows,
                        col_start_inclusive: col_start,
                        col_end_exclusive: col_end,
                    },
                    data_formats_by_col,
                    scientific_formats_by_col,
                    numeric_cols_idx: cols_idx_numeric_slice.iter().copied().collect(),
                    integer_cols_idx: cols_idx_integer_slice.iter().copied().collect(),
                    decimal_cols_idx: cols_idx_decimal_slice.iter().copied().collect(),
                    is_decimal_explicit: !cols_idx_decimal_slice.is_empty(),
                },
                key,
            });
            col_start = col_end;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn finalize_single_pass_sheet(
        &mut self,
        sheet_name: &str,
        rows_written: usize,
        plan: &XlsxSinglePassPlan,
        options: &XlsxSheetWriteOptions,
        tracker: &LogicalAutofitTracker,
        registry: PhysicalSheetRegistry,
        worksheet_segment_start: usize,
        report: &mut XlsxReport,
    ) -> Result<(), String> {
        let planned = plan_sheet_slices(
            rows_written,
            plan.col_names.len(),
            plan.header_grid.len(),
            &sanitize_sheet_name(sheet_name, "_"),
            report,
        )?;
        let entries = registry.into_canonical(&planned)?;
        let worksheet_segment_end = worksheet_segment_start + entries.len();
        if worksheet_segment_end != self.workbook.worksheets().len() {
            return Err(format!(
                "Physical worksheet segment mismatch: expected [{worksheet_segment_start}, {worksheet_segment_end}) but workbook has {} sheets.",
                self.workbook.worksheets().len()
            ));
        }
        let actual_indices = entries
            .iter()
            .map(|entry| entry.worksheet_index)
            .collect::<BTreeSet<_>>();
        let expected_indices =
            (worksheet_segment_start..worksheet_segment_end).collect::<BTreeSet<_>>();
        if actual_indices != expected_indices {
            return Err(format!(
                "Physical worksheet indices are not the appended segment: {actual_indices:?}."
            ));
        }

        for (sheet, entry) in planned.iter().zip(&entries) {
            #[cfg(test)]
            self.fail_at_finalize_point(TestFinalizeFailurePoint::ApplyColumnWidths)?;
            let worksheet = self
                .workbook
                .worksheet_from_index(entry.worksheet_index)
                .map_err(format_xlsx_error_text)?;
            if worksheet.name() != entry.temporary_name {
                return Err(format!(
                    "Physical worksheet name mismatch at index {}: expected {:?}, got {:?}.",
                    entry.worksheet_index,
                    entry.temporary_name,
                    worksheet.name()
                ));
            }
            apply_column_widths(
                worksheet,
                &options.policy_autofit,
                &tracker.header_widths_by_col[sheet.col_start_inclusive..sheet.col_end_exclusive],
                &tracker.body_widths_by_col[sheet.col_start_inclusive..sheet.col_end_exclusive],
            )?;
        }

        let final_names = planned
            .iter()
            .map(|sheet| self.ensure_unique_sheet_name(&sheet.sheet_name))
            .collect::<Vec<_>>();
        let reserved_final_names = final_names.iter().cloned().collect::<BTreeSet<_>>();
        let mut occupied_names = self
            .workbook
            .worksheets()
            .iter()
            .map(|worksheet| worksheet.name())
            .collect::<BTreeSet<_>>();
        let mut next_stage_index = 1usize;
        #[cfg(test)]
        self.fail_at_finalize_point(TestFinalizeFailurePoint::RenameWorksheets)?;
        for entry in &entries {
            occupied_names.remove(&entry.temporary_name);
            let stage_name = allocate_internal_sheet_name(
                "stage",
                &mut next_stage_index,
                &occupied_names,
                &reserved_final_names,
            );
            self.workbook
                .worksheet_from_index(entry.worksheet_index)
                .map_err(format_xlsx_error_text)?
                .set_name(&stage_name)
                .map_err(format_xlsx_error_text)?;
            occupied_names.insert(stage_name);
        }
        for (entry, final_name) in entries.iter().zip(&final_names) {
            self.workbook
                .worksheet_from_index(entry.worksheet_index)
                .map_err(format_xlsx_error_text)?
                .set_name(final_name)
                .map_err(format_xlsx_error_text)?;
        }

        let rank_by_name = final_names
            .iter()
            .enumerate()
            .map(|(rank, name)| (name.clone(), rank))
            .collect::<BTreeMap<_, _>>();
        #[cfg(test)]
        self.fail_at_finalize_point(TestFinalizeFailurePoint::ReorderWorksheets)?;
        self.workbook.worksheets_mut()[worksheet_segment_start..worksheet_segment_end]
            .sort_by_key(|worksheet| rank_by_name.get(&worksheet.name()).copied());
        let reordered_names = self.workbook.worksheets()
            [worksheet_segment_start..worksheet_segment_end]
            .iter()
            .map(|worksheet| worksheet.name())
            .collect::<Vec<_>>();
        if reordered_names != final_names {
            return Err(format!(
                "Physical worksheet reorder mismatch: expected {final_names:?}, got {reordered_names:?}."
            ));
        }

        #[cfg(test)]
        self.fail_at_finalize_point(TestFinalizeFailurePoint::ConstructReport)?;
        report.sheets = planned
            .into_iter()
            .zip(final_names)
            .map(|(mut sheet, final_name)| {
                sheet.sheet_name = final_name;
                sheet
            })
            .collect();
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn write_arrow_record_batch_to_runtime_sheet(
    workbook: &mut Workbook,
    runtime: &XlsxSheetRuntime,
    batch: &XlsxRecordBatch,
    row_offset: usize,
    header_row_count: usize,
    should_keep_missing_values: bool,
    value_policy: &XlsxValuePolicy,
    policy_scientific: &ScientificPolicy,
    value_plans: &[crate::spec::ColumnValuePlan],
    tracker: &mut LogicalAutofitTracker,
    report: &mut XlsxReport,
) -> Result<(), String> {
    let batch_start = row_offset;
    let batch_end = row_offset + batch.len();
    let sheet_start = runtime.sheet_slice.row_start_inclusive;
    let sheet_end = runtime.sheet_slice.row_end_exclusive;
    let overlap_start = batch_start.max(sheet_start);
    let overlap_end = batch_end.min(sheet_end);
    if overlap_start >= overlap_end {
        return Ok(());
    }

    let worksheet = workbook
        .worksheet_from_index(runtime.worksheet_index)
        .map_err(format_xlsx_error_text)?;
    for row_abs in overlap_start..overlap_end {
        let row_local_in_batch = row_abs - batch_start;
        let row_local_in_sheet = row_abs - sheet_start;
        for col_abs in
            runtime.sheet_slice.col_start_inclusive..runtime.sheet_slice.col_end_exclusive
        {
            let col_idx = col_abs - runtime.sheet_slice.col_start_inclusive;
            let is_numeric_col = runtime.numeric_cols_idx.contains(&col_idx);
            let is_integer_col = runtime.integer_cols_idx.contains(&col_idx);
            let is_decimal_specified = runtime.decimal_cols_idx.contains(&col_idx);
            let normalized = normalize_arrow_cell(
                batch.arrays()[col_abs].as_ref(),
                row_local_in_batch,
                value_plans.get(col_abs),
                is_numeric_col,
                is_integer_col,
                runtime.is_decimal_explicit,
                is_decimal_specified,
                should_keep_missing_values,
                value_policy,
                policy_scientific,
            )?;
            if let Some(warning) = normalized.warning
                && let Some(plan) = value_plans.get(col_abs)
            {
                add_conversion_warning(report, col_abs, &plan.name, warning);
            }
            tracker.observe(
                row_abs,
                col_abs,
                &normalized,
                should_keep_missing_values,
                value_policy,
            );
            let fmt_cell = if normalized.should_use_scientific {
                &runtime.scientific_formats_by_col[col_idx]
            } else {
                &runtime.data_formats_by_col[col_idx]
            };
            write_cell_with_format(
                worksheet,
                header_row_count + row_local_in_sheet,
                col_idx,
                &normalized.value,
                fmt_cell,
            )?;
        }
    }
    Ok(())
}

fn calculate_slice_indices(
    indices: &[usize],
    col_start_inclusive: usize,
    col_end_exclusive: usize,
) -> Vec<usize> {
    indices
        .iter()
        .filter(|idx| **idx >= col_start_inclusive && **idx < col_end_exclusive)
        .map(|idx| *idx - col_start_inclusive)
        .collect()
}

fn allocate_internal_sheet_name(
    phase: &str,
    next_index: &mut usize,
    occupied: &BTreeSet<String>,
    reserved: &BTreeSet<String>,
) -> String {
    loop {
        let candidate = format!("__nx_{phase}_{}", *next_index);
        *next_index += 1;
        if !occupied.contains(&candidate) && !reserved.contains(&candidate) {
            return candidate;
        }
    }
}

fn add_conversion_warning(
    report: &mut XlsxReport,
    column_index: usize,
    column_name: &str,
    warning: WarningCode,
) {
    let (prefix, detail) = match warning {
        WarningCode::PrecisionAsText => (
            "[precision-as-text]",
            "contains values beyond Excel's 15-digit numeric precision; affected cells were written as exact text.",
        ),
        WarningCode::TemporalAsText => (
            "[temporal-as-text]",
            "contains values outside Excel's lossless temporal serial range; affected cells were written as exact text.",
        ),
    };
    let key = format!("{prefix} column {column_name:?}");
    if report.warnings.iter().any(|item| item.starts_with(&key)) {
        return;
    }
    report
        .warnings
        .push(format!("{key} {detail} (source column {column_index})"));
}

fn sort_conversion_warnings(report: &mut XlsxReport) {
    report.warnings.sort_by_key(|warning| {
        warning
            .rsplit_once("(source column ")
            .and_then(|(_, suffix)| suffix.strip_suffix(')'))
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(usize::MAX)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::PrimitiveArray;
    use arrow::datatypes::{ArrowDataType, ArrowSchema, Field};
    use std::sync::Arc;

    use crate::spec::{CellFormatPatch, CellValue, XlsxValuePolicy, XlsxWriteOptions};

    fn normalized(value: &str) -> NormalizedCell {
        NormalizedCell {
            value: CellValue::String(value.to_string()),
            warning: None,
            is_numeric_col: false,
            is_integer_col: false,
            should_use_scientific: false,
        }
    }

    fn writer_for_finalization_failure(point: TestFinalizeFailurePoint) -> XlsxWriter {
        let mut writer = XlsxWriter::new(
            std::env::temp_dir().join(format!("neatxlsx-core-finalize-{point:?}.xlsx")),
            CellFormatPatch::default(),
            CellFormatPatch::default(),
            CellFormatPatch::default(),
            CellFormatPatch::default(),
            CellFormatPatch::default(),
            XlsxWriteOptions::default(),
        )
        .unwrap();
        writer.inject_finalize_failure(point);
        writer
    }

    fn one_column_batch() -> XlsxRecordBatch {
        let schema = Arc::new(ArrowSchema::from_iter([Field::new(
            "value".into(),
            ArrowDataType::Int64,
            false,
        )]));
        let values: Box<dyn ArrowArray> = Box::new(PrimitiveArray::<i64>::from_vec(vec![42]));
        XlsxRecordBatch::new(1, schema, vec![values])
    }

    #[test]
    fn autofit_tracker_uses_absolute_row_limit_across_batches() {
        let policy = AutofitPolicy {
            mode: AutofitMode::Body,
            height_body_inferred_max: Some(2),
            ..AutofitPolicy::default()
        };
        let value_policy = XlsxValuePolicy::default();
        let mut tracker =
            LogicalAutofitTracker::new(1, &[vec!["header".into()]], &policy, false, &value_policy);

        tracker.observe(0, 0, &normalized("a"), false, &value_policy);
        tracker.observe(1, 0, &normalized("included"), false, &value_policy);
        tracker.observe(
            2,
            0,
            &normalized("excluded-and-longer"),
            false,
            &value_policy,
        );

        assert_eq!(tracker.header_widths_by_col, [0]);
        assert_eq!(tracker.body_widths_by_col, [8]);
    }

    #[test]
    fn registry_matches_stable_starts() {
        let planned = vec![SheetSlice {
            sheet_name: "Data".to_string(),
            row_start_inclusive: 0,
            row_end_exclusive: 3,
            col_start_inclusive: 0,
            col_end_exclusive: 2,
        }];
        let key = PhysicalSheetKey::from_slice(&planned[0]);
        let mut registry = PhysicalSheetRegistry::default();
        registry
            .register(
                key,
                PhysicalSheetEntry {
                    worksheet_index: 0,
                    temporary_name: "__nx_tmp_1".to_string(),
                    actual_row_end: 3,
                },
            )
            .unwrap();

        let entries = registry.into_canonical(&planned).unwrap();
        assert_eq!(entries[0].worksheet_index, 0);
    }

    fn planned_slice(
        sheet_name: &str,
        row_start_inclusive: usize,
        row_end_exclusive: usize,
        col_start_inclusive: usize,
        col_end_exclusive: usize,
    ) -> SheetSlice {
        SheetSlice {
            sheet_name: sheet_name.to_string(),
            row_start_inclusive,
            row_end_exclusive,
            col_start_inclusive,
            col_end_exclusive,
        }
    }

    fn registry_entry(index: usize, name: &str, row_end: usize) -> PhysicalSheetEntry {
        PhysicalSheetEntry {
            worksheet_index: index,
            temporary_name: name.to_string(),
            actual_row_end: row_end,
        }
    }

    #[test]
    fn registry_rejects_missing_planned_key() {
        let planned = vec![planned_slice("Data", 0, 3, 0, 2)];

        let error = PhysicalSheetRegistry::default()
            .into_canonical(&planned)
            .unwrap_err();

        assert!(error.contains("Missing physical worksheet registry key"));
    }

    #[test]
    fn registry_rejects_orphan_key() {
        let planned = vec![planned_slice("Data", 0, 3, 0, 2)];
        let orphan = planned_slice("Data_2", 3, 6, 0, 2);
        let mut registry = PhysicalSheetRegistry::default();
        registry
            .register(
                PhysicalSheetKey::from_slice(&planned[0]),
                registry_entry(0, "__nx_tmp_1", 3),
            )
            .unwrap();
        registry
            .register(
                PhysicalSheetKey::from_slice(&orphan),
                registry_entry(1, "__nx_tmp_2", 6),
            )
            .unwrap();

        let error = registry.into_canonical(&planned).unwrap_err();

        assert!(error.contains("Orphan physical worksheet registry keys"));
    }

    #[test]
    fn registry_rejects_row_end_mismatch() {
        let planned = vec![planned_slice("Data", 0, 3, 0, 2)];
        let mut registry = PhysicalSheetRegistry::default();
        registry
            .register(
                PhysicalSheetKey::from_slice(&planned[0]),
                registry_entry(0, "__nx_tmp_1", 2),
            )
            .unwrap();

        let error = registry.into_canonical(&planned).unwrap_err();

        assert!(error.contains("Physical worksheet row boundary mismatch"));
    }

    #[test]
    fn registry_returns_entries_in_planned_canonical_order() {
        let planned = vec![
            planned_slice("Data", 0, 2, 0, 2),
            planned_slice("Data_2", 2, 4, 0, 2),
            planned_slice("Data_3", 0, 2, 2, 4),
            planned_slice("Data_4", 2, 4, 2, 4),
        ];
        let mut registry = PhysicalSheetRegistry::default();
        // Runtime creation is row-first: r0c0, r0c1, r1c0, r1c1. The static
        // planner is column-first: r0c0, r1c0, r0c1, r1c1.
        for (index, slice) in
            [0usize, 1, 2, 3]
                .into_iter()
                .zip([&planned[0], &planned[2], &planned[1], &planned[3]])
        {
            registry
                .register(
                    PhysicalSheetKey::from_slice(slice),
                    registry_entry(index, &format!("__nx_tmp_{index}"), slice.row_end_exclusive),
                )
                .unwrap();
        }

        let entries = registry.into_canonical(&planned).unwrap();

        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.worksheet_index)
                .collect::<Vec<_>>(),
            vec![0, 2, 1, 3]
        );
    }

    #[test]
    fn internal_names_skip_temporary_and_reserved_final_names() {
        let occupied = BTreeSet::from(["__nx_tmp_1".to_string(), "__nx_tmp_3".to_string()]);
        let reserved = BTreeSet::from(["__nx_tmp_2".to_string()]);
        let mut next_index = 1;

        let name = allocate_internal_sheet_name("tmp", &mut next_index, &occupied, &reserved);

        assert_eq!(name, "__nx_tmp_4");
        assert_eq!(next_index, 5);
    }

    #[test]
    fn staging_names_skip_reserved_final_names() {
        let occupied = BTreeSet::from(["__nx_stage_1".to_string()]);
        let reserved = BTreeSet::from(["__nx_stage_2".to_string()]);
        let mut next_index = 1;

        let name = allocate_internal_sheet_name("stage", &mut next_index, &occupied, &reserved);

        assert_eq!(name, "__nx_stage_3");
    }

    #[test]
    fn slice_indices_do_not_rebase_columns_before_the_slice() {
        assert_eq!(calculate_slice_indices(&[0, 16_384], 16_384, 16_385), [0]);
    }

    #[test]
    fn finalization_failures_poison_writer_without_appending_report() {
        for point in [
            TestFinalizeFailurePoint::ApplyColumnWidths,
            TestFinalizeFailurePoint::RenameWorksheets,
            TestFinalizeFailurePoint::ReorderWorksheets,
            TestFinalizeFailurePoint::ConstructReport,
        ] {
            let mut writer = writer_for_finalization_failure(point);
            let options = XlsxSheetWriteOptions::default();

            let error = writer
                .write_sheet_from_record_batch_results(
                    vec![Ok(one_column_batch())],
                    "Data",
                    None,
                    &options,
                )
                .unwrap_err();

            assert!(
                error.contains("Injected finalization failure"),
                "unexpected error for {point:?}: {error}"
            );
            assert!(writer.report().is_empty(), "report leaked for {point:?}");
            assert_eq!(
                writer
                    .write_sheet_from_record_batch_results(
                        vec![Ok(one_column_batch())],
                        "Again",
                        None,
                        &options,
                    )
                    .unwrap_err(),
                "Cannot use a poisoned workbook."
            );
            assert_eq!(
                writer.close().unwrap_err(),
                "Cannot close a poisoned workbook."
            );
        }
    }
}
