//! RecordBatch planning and one-pass/two-pass streaming orchestration.

use std::collections::{BTreeMap, BTreeSet};

use arrow::array::Array as ArrowArray;
use arrow::record_batch::RecordBatchT;
use rust_xlsxwriter::{Format, Workbook};

use crate::constant::{LEN_SHEET_NAME_MAX, NCOLS_SHEET_MAX, NROWS_SHEET_MAX};
use crate::spec::{
    AutofitMode, CellValue, ColumnValueKind, ScientificPolicy, SheetSlice, WarningCode, XlsxReport,
    XlsxValuePolicy,
};
use crate::util::{
    calculate_row_chunk_size, convert_cell_value, sanitize_sheet_name,
    select_sorted_indices_from_refs, validate_unique_columns,
};

use super::plan::{XlsxSheetPlan, XlsxSheetPlanBuilder, calculate_slice_indices};
use super::render::{
    ColumnFormatPlanOptions, apply_column_widths, cast_col_num, cast_row_num,
    create_rust_xlsx_format, format_xlsx_error_text, inferred_num_formats, plan_column_formats,
    write_cell_with_format, write_header,
};
use super::value::{
    convert_arrow_value_with_plan, estimate_width_len, is_scientific_candidate_col,
    select_integer_column_indices_from_arrow_schema,
    select_numeric_column_indices_from_arrow_schema, should_use_scientific_value,
    validate_policy_autofit, validate_policy_scientific,
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
    fmt_scientific: Format,
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
    header_widths_by_col: Vec<usize>,
    body_widths_by_col: Vec<usize>,
    num_frozen_rows: usize,
    should_keep_missing_values: bool,
}

struct XlsxSinglePassRuntimeSheet {
    runtime: XlsxSheetRuntime,
    report_index: usize,
}

impl XlsxWriter {
    /// Plan one sheet from record batches without materializing the full body.
    pub fn plan_sheet_from_record_batches<I>(
        &self,
        batches: I,
        sheet_name: &str,
        header_grid: Option<Vec<Vec<String>>>,
        options: &XlsxSheetWriteOptions,
    ) -> Result<XlsxSheetPlan, String>
    where
        I: IntoIterator<Item = XlsxRecordBatch>,
    {
        self.plan_sheet_from_record_batch_results(
            batches.into_iter().map(Ok),
            sheet_name,
            header_grid,
            options,
        )
    }

    /// Plan one sheet from fallible record batch stream without materializing the full body.
    pub fn plan_sheet_from_record_batch_results<I>(
        &self,
        batches: I,
        sheet_name: &str,
        header_grid: Option<Vec<Vec<String>>>,
        options: &XlsxSheetWriteOptions,
    ) -> Result<XlsxSheetPlan, String>
    where
        I: IntoIterator<Item = XlsxRecordBatchResult>,
    {
        if self.is_closed {
            return Err("Cannot write after close().".to_string());
        }
        validate_policy_autofit(&options.policy_autofit)?;
        validate_policy_scientific(&options.policy_scientific)?;

        let mut builder =
            XlsxSheetPlanBuilder::new(sheet_name, header_grid, options, &self.options_write);
        for batch in batches {
            builder.scan_batch(batch?)?;
        }
        builder.finish()
    }

    /// Write one sheet from record batches using a precomputed streaming plan.
    pub fn write_sheet_from_record_batches<I>(
        &mut self,
        plan: XlsxSheetPlan,
        batches: I,
        options: &XlsxSheetWriteOptions,
    ) -> Result<(), String>
    where
        I: IntoIterator<Item = XlsxRecordBatch>,
    {
        self.write_sheet_from_record_batch_results(plan, batches.into_iter().map(Ok), options)
    }

    /// Write one sheet from fallible record batch stream using a precomputed streaming plan.
    pub fn write_sheet_from_record_batch_results<I>(
        &mut self,
        plan: XlsxSheetPlan,
        batches: I,
        options: &XlsxSheetWriteOptions,
    ) -> Result<(), String>
    where
        I: IntoIterator<Item = XlsxRecordBatchResult>,
    {
        if self.is_closed {
            return Err("Cannot write after close().".to_string());
        }
        self.write_sheet_record_batches(plan, batches, options)
    }

    /// Write one sheet from fallible record batch stream in one pass.
    ///
    /// This path is only valid when column widths don't require body pre-scan.
    pub fn write_sheet_from_record_batch_results_single_pass<I>(
        &mut self,
        batches: I,
        sheet_name: &str,
        header_grid: Option<Vec<Vec<String>>>,
        options: &XlsxSheetWriteOptions,
    ) -> Result<(), String>
    where
        I: IntoIterator<Item = XlsxRecordBatchResult>,
    {
        if self.is_closed {
            return Err("Cannot write after close().".to_string());
        }
        validate_policy_autofit(&options.policy_autofit)?;
        validate_policy_scientific(&options.policy_scientific)?;
        if matches!(
            options.policy_autofit.mode,
            AutofitMode::Body | AutofitMode::All
        ) {
            return Err(
                "single-pass XLSX writing requires policy_autofit.mode to be 'header' or 'none'."
                    .to_string(),
            );
        }
        self.write_sheet_record_batches_single_pass(batches, sheet_name, header_grid, options)
    }

    fn write_sheet_record_batches<I>(
        &mut self,
        plan: XlsxSheetPlan,
        batches: I,
        options: &XlsxSheetWriteOptions,
    ) -> Result<(), String>
    where
        I: IntoIterator<Item = XlsxRecordBatchResult>,
    {
        let col_names_ref = plan
            .col_names
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let header_row_count = plan.header_grid.len();
        let value_policy = self.options_write.value_policy.clone();

        let mut report = XlsxReport {
            sheets: vec![],
            warnings: vec![],
        };
        let mut runtime_sheets = Vec::with_capacity(plan.sheet_slices.len());

        for sheet_slice in &plan.sheet_slices {
            let sheet_name_unique = self.ensure_unique_sheet_name(&sheet_slice.sheet_name);
            let worksheet_index = self.workbook.worksheets().len();
            let worksheet = self.workbook.add_worksheet_with_constant_memory();
            worksheet
                .set_name(&sheet_name_unique)
                .map_err(format_xlsx_error_text)?;

            let cols_idx_numeric_slice = calculate_slice_indices(
                &plan.cols_idx_numeric,
                sheet_slice.col_start_inclusive,
                sheet_slice.col_end_exclusive,
            );
            let cols_idx_integer_slice = calculate_slice_indices(
                &plan.cols_idx_integer,
                sheet_slice.col_start_inclusive,
                sheet_slice.col_end_exclusive,
            );
            let cols_idx_decimal_slice = calculate_slice_indices(
                &plan.cols_idx_decimal_specified,
                sheet_slice.col_start_inclusive,
                sheet_slice.col_end_exclusive,
            );
            let inferred_num_formats_all = inferred_num_formats(&options.value_plans);
            let inferred_num_formats_slice =
                if inferred_num_formats_all.len() >= sheet_slice.col_end_exclusive {
                    inferred_num_formats_all
                        [sheet_slice.col_start_inclusive..sheet_slice.col_end_exclusive]
                        .to_vec()
                } else {
                    vec![None; sheet_slice.col_end_exclusive - sheet_slice.col_start_inclusive]
                };

            let column_format_plan = plan_column_formats(ColumnFormatPlanOptions {
                width_data: sheet_slice.col_end_exclusive - sheet_slice.col_start_inclusive,
                cols_idx_numeric: &cols_idx_numeric_slice,
                cols_idx_integer: &cols_idx_integer_slice,
                cols_idx_decimal: if cols_idx_decimal_slice.is_empty() {
                    None
                } else {
                    Some(&cols_idx_decimal_slice)
                },
                cols_fmt_overrides: &BTreeMap::new(),
                fmt_text: &self.fmt_text,
                fmt_integer: &self.fmt_integer,
                fmt_decimal: &self.fmt_decimal,
                fmt_text_override: &self.fmt_text_override,
                fmt_integer_override: &self.fmt_integer_override,
                fmt_decimal_override: &self.fmt_decimal_override,
                inferred_num_formats: Some(&inferred_num_formats_slice),
            });

            let data_formats_by_col: Vec<Format> = column_format_plan
                .fmts_by_col
                .iter()
                .map(create_rust_xlsx_format)
                .collect();
            let fmt_scientific = create_rust_xlsx_format(&self.fmt_scientific);
            let fmt_header = create_rust_xlsx_format(&self.fmt_header);

            let header_grid_slice = plan
                .header_grid
                .iter()
                .map(|row| {
                    row[sheet_slice.col_start_inclusive..sheet_slice.col_end_exclusive].to_vec()
                })
                .collect::<Vec<_>>();

            write_header(
                worksheet,
                header_grid_slice,
                options.should_merge_header,
                &fmt_header,
            )?;

            worksheet
                .set_freeze_panes(
                    cast_row_num(plan.num_frozen_rows)?,
                    cast_col_num(options.num_frozen_cols)?,
                )
                .map_err(format_xlsx_error_text)?;

            apply_column_widths(
                worksheet,
                &options.policy_autofit,
                &plan.header_widths_by_col
                    [sheet_slice.col_start_inclusive..sheet_slice.col_end_exclusive],
                &plan.body_widths_by_col
                    [sheet_slice.col_start_inclusive..sheet_slice.col_end_exclusive],
            )?;

            runtime_sheets.push(XlsxSheetRuntime {
                worksheet_index,
                sheet_slice: sheet_slice.clone(),
                data_formats_by_col,
                fmt_scientific,
                numeric_cols_idx: cols_idx_numeric_slice.iter().copied().collect(),
                integer_cols_idx: cols_idx_integer_slice.iter().copied().collect(),
                decimal_cols_idx: cols_idx_decimal_slice.iter().copied().collect(),
                is_decimal_explicit: !cols_idx_decimal_slice.is_empty(),
            });

            report.sheets.push(SheetSlice {
                sheet_name: sheet_name_unique,
                row_start_inclusive: sheet_slice.row_start_inclusive,
                row_end_exclusive: sheet_slice.row_end_exclusive,
                col_start_inclusive: sheet_slice.col_start_inclusive,
                col_end_exclusive: sheet_slice.col_end_exclusive,
            });
        }

        let mut row_offset = 0usize;
        for batch in batches {
            let batch = batch?;
            let batch_col_names = batch
                .schema()
                .iter_names()
                .map(|name| name.as_str())
                .collect::<Vec<_>>();
            if batch_col_names != col_names_ref {
                return Err("All record batches must have identical column names.".to_string());
            }

            for runtime in &runtime_sheets {
                write_arrow_record_batch_to_runtime_sheet(
                    &mut self.workbook,
                    runtime,
                    &batch,
                    row_offset,
                    header_row_count,
                    plan.should_keep_missing_values,
                    &value_policy,
                    &options.policy_scientific,
                    &options.value_plans,
                    &mut report,
                )?;
            }
            row_offset += batch.len();
        }

        if row_offset != plan.height_body {
            return Err(format!(
                "Streaming write row count mismatch: planned {} rows but wrote {row_offset}.",
                plan.height_body
            ));
        }

        sort_conversion_warnings(&mut report);
        self.reports.push(report);
        Ok(())
    }

    fn write_sheet_record_batches_single_pass<I>(
        &mut self,
        batches: I,
        sheet_name: &str,
        header_grid_custom: Option<Vec<Vec<String>>>,
        options: &XlsxSheetWriteOptions,
    ) -> Result<(), String>
    where
        I: IntoIterator<Item = XlsxRecordBatchResult>,
    {
        let mut iter_batches = batches.into_iter();
        let Some(first_batch_result) = iter_batches.next() else {
            return Err(
                "Cannot write sheet from an empty batch stream with unknown schema.".to_string(),
            );
        };
        let first_batch = first_batch_result?;
        let plan =
            self.create_single_pass_plan(&first_batch, sheet_name, header_grid_custom, options)?;
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
        let mut runtime_sheets: Vec<XlsxSinglePassRuntimeSheet> = vec![];
        let mut active_row_start: Option<usize> = None;
        let mut next_part_idx = 1usize;
        let mut rows_written = 0usize;

        self.write_single_pass_batch(
            &plan,
            options,
            sheet_name,
            &first_batch,
            &col_names_ref,
            rows_written,
            max_data_rows,
            &mut active_row_start,
            &mut next_part_idx,
            &mut runtime_sheets,
            &mut report,
            &options.value_plans,
        )?;
        rows_written += first_batch.len();

        for batch in iter_batches {
            let batch = batch?;
            self.write_single_pass_batch(
                &plan,
                options,
                sheet_name,
                &batch,
                &col_names_ref,
                rows_written,
                max_data_rows,
                &mut active_row_start,
                &mut next_part_idx,
                &mut runtime_sheets,
                &mut report,
                &options.value_plans,
            )?;
            rows_written += batch.len();
        }

        if rows_written == 0 {
            self.ensure_single_pass_runtime_sheets(
                &plan,
                options,
                sheet_name,
                0,
                max_data_rows,
                &mut active_row_start,
                &mut next_part_idx,
                &mut runtime_sheets,
                &mut report,
            )?;
        }

        sort_conversion_warnings(&mut report);
        self.reports.push(report);
        Ok(())
    }

    fn create_single_pass_plan(
        &self,
        first_batch: &XlsxRecordBatch,
        _sheet_name: &str,
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
        let mut header_widths_by_col = vec![0usize; width_body];
        let body_widths_by_col = vec![0usize; width_body];
        if !matches!(options.policy_autofit.mode, AutofitMode::None) {
            for col_idx in 0..width_body {
                for row in &header_grid {
                    let value = &row[col_idx];
                    if value.is_empty() {
                        continue;
                    }
                    header_widths_by_col[col_idx] = usize::max(
                        header_widths_by_col[col_idx],
                        estimate_width_len(
                            &CellValue::String(value.clone()),
                            false,
                            false,
                            false,
                            &options.policy_scientific,
                            should_keep_missing_values,
                            &self.options_write.value_policy,
                        ),
                    );
                }
            }
        }
        let header_row_count = header_grid.len();

        Ok(XlsxSinglePassPlan {
            col_names,
            header_grid,
            cols_idx_numeric,
            cols_idx_integer,
            cols_idx_decimal_specified,
            header_widths_by_col,
            body_widths_by_col,
            num_frozen_rows: options.num_frozen_rows.unwrap_or(header_row_count),
            should_keep_missing_values,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn write_single_pass_batch(
        &mut self,
        plan: &XlsxSinglePassPlan,
        options: &XlsxSheetWriteOptions,
        sheet_name: &str,
        batch: &XlsxRecordBatch,
        col_names_ref: &[&str],
        row_offset: usize,
        max_data_rows: usize,
        active_row_start: &mut Option<usize>,
        next_part_idx: &mut usize,
        runtime_sheets: &mut Vec<XlsxSinglePassRuntimeSheet>,
        report: &mut XlsxReport,
        value_plans: &[crate::spec::ColumnValuePlan],
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
                sheet_name,
                row_part_start,
                max_data_rows,
                active_row_start,
                next_part_idx,
                runtime_sheets,
                report,
            )?;

            for runtime in runtime_sheets.iter_mut() {
                write_arrow_record_batch_to_runtime_sheet(
                    &mut self.workbook,
                    &runtime.runtime,
                    batch,
                    row_offset,
                    plan.header_grid.len(),
                    plan.should_keep_missing_values,
                    &self.options_write.value_policy,
                    &options.policy_scientific,
                    value_plans,
                    report,
                )?;
                let report_sheet = &mut report.sheets[runtime.report_index];
                let overlap_end =
                    usize::min(batch_end, runtime.runtime.sheet_slice.row_end_exclusive);
                if overlap_end > report_sheet.row_end_exclusive {
                    report_sheet.row_end_exclusive = overlap_end;
                }
            }

            segment_start = usize::min(batch_end, row_part_start + max_data_rows);
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn ensure_single_pass_runtime_sheets(
        &mut self,
        plan: &XlsxSinglePassPlan,
        options: &XlsxSheetWriteOptions,
        sheet_name: &str,
        row_part_start: usize,
        max_data_rows: usize,
        active_row_start: &mut Option<usize>,
        next_part_idx: &mut usize,
        runtime_sheets: &mut Vec<XlsxSinglePassRuntimeSheet>,
        report: &mut XlsxReport,
    ) -> Result<(), String> {
        if active_row_start.is_some_and(|value| value == row_part_start) {
            return Ok(());
        }

        runtime_sheets.clear();
        *active_row_start = Some(row_part_start);

        let width_body = plan.col_names.len();
        let mut col_start = 0usize;
        let has_multiple_col_parts = width_body > NCOLS_SHEET_MAX;
        while col_start < width_body {
            let col_end = usize::min(width_body, col_start + NCOLS_SHEET_MAX);
            let sheet_name_base = sanitize_sheet_name(sheet_name, "_");
            let sheet_name_planned = if *next_part_idx == 1 && !has_multiple_col_parts {
                sheet_name_base
            } else {
                create_sheet_identifier_local(&sheet_name_base, *next_part_idx)
            };
            *next_part_idx += 1;

            let sheet_name_unique = self.ensure_unique_sheet_name(&sheet_name_planned);
            let worksheet_index = self.workbook.worksheets().len();
            let worksheet = self.workbook.add_worksheet_with_constant_memory();
            worksheet
                .set_name(&sheet_name_unique)
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
            let column_format_plan = plan_column_formats(ColumnFormatPlanOptions {
                width_data: col_end - col_start,
                cols_idx_numeric: &cols_idx_numeric_slice,
                cols_idx_integer: &cols_idx_integer_slice,
                cols_idx_decimal: if cols_idx_decimal_slice.is_empty() {
                    None
                } else {
                    Some(&cols_idx_decimal_slice)
                },
                cols_fmt_overrides: &BTreeMap::new(),
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
            let fmt_scientific = create_rust_xlsx_format(&self.fmt_scientific);
            let fmt_header = create_rust_xlsx_format(&self.fmt_header);
            let header_grid_slice = plan
                .header_grid
                .iter()
                .map(|row| row[col_start..col_end].to_vec())
                .collect::<Vec<_>>();
            write_header(
                worksheet,
                header_grid_slice,
                options.should_merge_header,
                &fmt_header,
            )?;
            worksheet
                .set_freeze_panes(
                    cast_row_num(plan.num_frozen_rows)?,
                    cast_col_num(options.num_frozen_cols)?,
                )
                .map_err(format_xlsx_error_text)?;
            apply_column_widths(
                worksheet,
                &options.policy_autofit,
                &plan.header_widths_by_col[col_start..col_end],
                &plan.body_widths_by_col[col_start..col_end],
            )?;

            let report_index = report.sheets.len();
            report.sheets.push(SheetSlice {
                sheet_name: sheet_name_unique,
                row_start_inclusive: row_part_start,
                row_end_exclusive: row_part_start,
                col_start_inclusive: col_start,
                col_end_exclusive: col_end,
            });

            runtime_sheets.push(XlsxSinglePassRuntimeSheet {
                runtime: XlsxSheetRuntime {
                    worksheet_index,
                    sheet_slice: SheetSlice {
                        sheet_name: sheet_name_planned,
                        row_start_inclusive: row_part_start,
                        row_end_exclusive: row_part_start + max_data_rows,
                        col_start_inclusive: col_start,
                        col_end_exclusive: col_end,
                    },
                    data_formats_by_col,
                    fmt_scientific,
                    numeric_cols_idx: cols_idx_numeric_slice.iter().copied().collect(),
                    integer_cols_idx: cols_idx_integer_slice.iter().copied().collect(),
                    decimal_cols_idx: cols_idx_decimal_slice.iter().copied().collect(),
                    is_decimal_explicit: !cols_idx_decimal_slice.is_empty(),
                },
                report_index,
            });

            col_start = col_end;
        }

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
    report: &mut XlsxReport,
) -> Result<(), String> {
    let batch_start = row_offset;
    let batch_end = row_offset + batch.len();
    let sheet_start = runtime.sheet_slice.row_start_inclusive;
    let sheet_end = runtime.sheet_slice.row_end_exclusive;
    let overlap_start = usize::max(batch_start, sheet_start);
    let overlap_end = usize::min(batch_end, sheet_end);
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
            let col = &batch.arrays()[col_abs];
            let is_numeric_col = runtime.numeric_cols_idx.contains(&col_idx);
            let is_integer_col = runtime.integer_cols_idx.contains(&col_idx);
            let is_decimal_specified = runtime.decimal_cols_idx.contains(&col_idx);
            let is_scientific_candidate = is_scientific_candidate_col(
                policy_scientific,
                is_integer_col,
                runtime.is_decimal_explicit,
                is_decimal_specified,
            );
            let value_raw = convert_arrow_value_with_plan(
                col.as_ref(),
                row_local_in_batch,
                value_plans.get(col_abs),
            )?;
            if let Some(warning) = value_raw.warning
                && let Some(plan) = value_plans.get(col_abs)
            {
                add_conversion_warning(report, col_abs, &plan.name, warning);
            }
            let value = if value_raw.warning.is_some() {
                value_raw.value
            } else {
                match value_plans.get(col_abs).map(|plan| plan.kind) {
                    Some(
                        ColumnValueKind::Boolean
                        | ColumnValueKind::Integer
                        | ColumnValueKind::Decimal
                        | ColumnValueKind::Date
                        | ColumnValueKind::Datetime
                        | ColumnValueKind::Time
                        | ColumnValueKind::Duration,
                    ) => {
                        if matches!(value_raw.value, CellValue::Blank) && should_keep_missing_values
                        {
                            CellValue::String(value_policy.missing_value_str.clone())
                        } else {
                            value_raw.value
                        }
                    }
                    Some(ColumnValueKind::Float) => convert_cell_value(
                        &value_raw.value,
                        true,
                        false,
                        should_keep_missing_values,
                        value_policy,
                    ),
                    _ => convert_cell_value(
                        &value_raw.value,
                        is_numeric_col,
                        is_integer_col,
                        should_keep_missing_values,
                        value_policy,
                    ),
                }
            };
            let should_use_scientific = should_use_scientific_value(
                &value,
                is_numeric_col,
                is_scientific_candidate,
                policy_scientific,
            );
            let fmt_cell = if should_use_scientific {
                &runtime.fmt_scientific
            } else {
                &runtime.data_formats_by_col[col_idx]
            };
            write_cell_with_format(
                worksheet,
                header_row_count + row_local_in_sheet,
                col_idx,
                &value,
                fmt_cell,
            )?;
        }
    }

    Ok(())
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

fn create_sheet_identifier_local(sheet_name: &str, part_idx: usize) -> String {
    let suffix = format!("__{part_idx}");
    let prefix_len = LEN_SHEET_NAME_MAX.saturating_sub(suffix.chars().count());
    let prefix = sheet_name.chars().take(prefix_len).collect::<String>();
    format!("{prefix}{suffix}")
}
