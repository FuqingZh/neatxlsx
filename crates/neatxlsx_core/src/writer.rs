//! XLSX writer kernel that converts DataFrame IPC into workbook output.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use std::path::PathBuf;

use arrow::array::{
    Array as ArrowArray, BooleanArray, PrimitiveArray, TryExtend, Utf8Array, Utf8ViewArray,
};
use arrow::datatypes::{ArrowDataType, ArrowSchema};
use arrow::record_batch::RecordBatchT;
use polars::prelude::{AnyValue, DataFrame, IpcReader, SerReader};
use rust_xlsxwriter::{Format, FormatAlign, FormatBorder, Workbook, Worksheet, XlsxError};

use crate::constant::{ColumnIdentifier, LEN_SHEET_NAME_MAX, NCOLS_SHEET_MAX, NROWS_SHEET_MAX};
use crate::spec::{
    AutofitMode, AutofitPolicy, CellFormatPatch, CellValue, ColumnFormatPlan, ScientificPolicy,
    ScientificScope, SheetSlice, XlsxReport, XlsxValuePolicy, XlsxWriteOptions,
};
use crate::util::{
    apply_vertical_run_text_blankout, calculate_row_chunk_size, convert_cell_value,
    create_horizontal_merge_tracker, generate_row_chunks, plan_horizontal_merges,
    plan_sheet_slices, sanitize_sheet_name, select_sorted_indices_from_refs,
    validate_unique_columns,
};

/// Per-sheet call options (aligned with Python `XlsxWriter.write_sheet` kwargs).
#[derive(Default, Debug, Clone)]
pub struct XlsxSheetWriteOptions {
    /// Integer columns by typed name or zero-based index.
    pub cols_integer: Option<Vec<ColumnIdentifier>>,
    /// Decimal columns by typed name or zero-based index.
    pub cols_decimal: Option<Vec<ColumnIdentifier>>,
    /// Number of frozen columns.
    pub num_frozen_cols: usize,
    /// Number of frozen top rows; defaults to header height when `None`.
    pub num_frozen_rows: Option<usize>,
    /// Enable merged multi-row header behavior.
    pub should_merge_header: bool,
    /// Override writer-level keep-missing behavior.
    pub should_keep_missing_values: Option<bool>,
    /// Column autofit policy.
    pub policy_autofit: AutofitPolicy,
    /// Scientific-format trigger policy.
    pub policy_scientific: ScientificPolicy,
}

struct ColumnFormatPlanOptions<'a> {
    /// Number of columns in current sheet slice.
    pub width_data: usize,
    /// Slice-local numeric column indices.
    pub cols_idx_numeric: &'a [usize],
    /// Slice-local integer column indices.
    pub cols_idx_integer: &'a [usize],
    /// Slice-local explicit decimal column indices.
    pub cols_idx_decimal: Option<&'a [usize]>,
    /// Optional per-column format overrides.
    pub cols_fmt_overrides: &'a BTreeMap<usize, CellFormatPatch>,
    /// Base text format.
    pub fmt_text: &'a CellFormatPatch,
    /// Base integer format.
    pub fmt_integer: &'a CellFormatPatch,
    /// Base decimal format.
    pub fmt_decimal: &'a CellFormatPatch,
    /// Global write options.
    pub options_write: &'a XlsxWriteOptions,
}

/// Arrow record batch shape accepted by the streaming writer.
pub type XlsxRecordBatch = RecordBatchT<Box<dyn ArrowArray>>;
/// Fallible Arrow record batch item accepted by bridge streaming sessions.
pub type XlsxRecordBatchResult = Result<XlsxRecordBatch, String>;

#[derive(Debug, Clone)]
pub struct XlsxSheetPlan {
    col_names: Vec<String>,
    height_body: usize,
    header_grid: Vec<Vec<String>>,
    cols_idx_numeric: Vec<usize>,
    cols_idx_integer: Vec<usize>,
    cols_idx_decimal_specified: Vec<usize>,
    header_widths_by_col: Vec<usize>,
    body_widths_by_col: Vec<usize>,
    sheet_slices: Vec<SheetSlice>,
    num_frozen_rows: usize,
    should_keep_missing_values: bool,
}

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

struct XlsxSheetPlanBuilder<'a> {
    sheet_name: &'a str,
    header_grid_custom: Option<Vec<Vec<String>>>,
    options: &'a XlsxSheetWriteOptions,
    options_write: &'a XlsxWriteOptions,
    value_policy: XlsxValuePolicy,
    col_names: Option<Vec<String>>,
    height_body: usize,
    width_body: usize,
    cols_idx_numeric: Vec<usize>,
    cols_idx_integer: Vec<usize>,
    cols_idx_decimal_specified: Vec<usize>,
    header_widths_by_col: Vec<usize>,
    body_widths_by_col: Vec<usize>,
    rows_seen_for_autofit: usize,
    should_keep_missing_values: bool,
}

/// Stateful workbook writer.
pub struct XlsxWriter {
    path_file_out: PathBuf,
    workbook: Workbook,
    fmt_text: CellFormatPatch,
    fmt_integer: CellFormatPatch,
    fmt_decimal: CellFormatPatch,
    fmt_scientific: CellFormatPatch,
    fmt_header: CellFormatPatch,
    options_write: XlsxWriteOptions,
    existing_sheet_names: BTreeSet<String>,
    reports: Vec<XlsxReport>,
    is_closed: bool,
}

impl XlsxWriter {
    /// Create writer bound to output path and format/options presets.
    ///
    /// The workbook is buffered in memory until [`Self::close`] is called.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        path_file_out: PathBuf,
        fmt_text: CellFormatPatch,
        fmt_integer: CellFormatPatch,
        fmt_decimal: CellFormatPatch,
        fmt_scientific: CellFormatPatch,
        fmt_header: CellFormatPatch,
        options_write: XlsxWriteOptions,
    ) -> Self {
        let mut workbook = Workbook::new();
        workbook.use_zip_large_file(options_write.should_use_zip64);

        Self {
            path_file_out,
            workbook,
            fmt_text,
            fmt_integer,
            fmt_decimal,
            fmt_scientific,
            fmt_header,
            options_write,
            existing_sheet_names: BTreeSet::new(),
            reports: Vec::new(),
            is_closed: false,
        }
    }

    /// Return output file path as string.
    pub fn file_out(&self) -> String {
        self.path_file_out.to_string_lossy().to_string()
    }

    /// Return immutable snapshot of per-sheet write reports.
    pub fn report(&self) -> Vec<XlsxReport> {
        self.reports.clone()
    }

    /// Flush workbook to disk. Idempotent.
    pub fn close(&mut self) -> Result<(), String> {
        if self.is_closed {
            return Ok(());
        }
        self.workbook
            .save(&self.path_file_out)
            .map_err(format_xlsx_error_text)?;
        self.is_closed = true;
        Ok(())
    }

    /// Write one sheet from in-memory dataframes.
    pub fn write_sheet_from_dataframes(
        &mut self,
        body: &DataFrame,
        sheet_name: &str,
        header: Option<&DataFrame>,
        options: &XlsxSheetWriteOptions,
    ) -> Result<(), String> {
        if self.is_closed {
            return Err("Cannot write after close().".to_string());
        }
        self.write_sheet(body, sheet_name, header, options)
    }

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

    /// Write one sheet from IPC-serialized dataframe bytes.
    ///
    /// `ipc_body` and optional `ipc_header` must be valid Polars IPC payloads.
    pub fn write_sheet_from_ipc_bytes(
        &mut self,
        ipc_body: &[u8],
        sheet_name: &str,
        ipc_header: Option<&[u8]>,
        options: &XlsxSheetWriteOptions,
    ) -> Result<(), String> {
        if self.is_closed {
            return Err("Cannot write after close().".to_string());
        }

        let df_body = read_dataframe_from_ipc_bytes(ipc_body)?;
        let header = match ipc_header {
            Some(val) => Some(read_dataframe_from_ipc_bytes(val)?),
            None => None,
        };
        self.write_sheet_from_dataframes(&df_body, sheet_name, header.as_ref(), options)
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
                options_write: &self.options_write,
            });

            let data_formats_by_col: Vec<Format> = column_format_plan
                .fmts_by_col
                .iter()
                .map(create_rust_xlsx_format)
                .collect();
            let fmt_scientific_patch = self
                .fmt_scientific
                .merge(&self.options_write.base_format_patch);
            let fmt_scientific = create_rust_xlsx_format(&fmt_scientific_patch);
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
            let df_batch = dataframe_from_record_batch(batch)?;
            let batch_col_names = df_batch.get_column_names_str();
            if batch_col_names != col_names_ref {
                return Err("All record batches must have identical column names.".to_string());
            }

            for runtime in &runtime_sheets {
                write_record_batch_to_runtime_sheet(
                    &mut self.workbook,
                    runtime,
                    &df_batch,
                    row_offset,
                    header_row_count,
                    plan.should_keep_missing_values,
                    &value_policy,
                    &options.policy_scientific,
                )?;
            }
            row_offset += df_batch.height();
        }

        if row_offset != plan.height_body {
            return Err(format!(
                "Streaming write row count mismatch: planned {} rows but wrote {row_offset}.",
                plan.height_body
            ));
        }

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

        let cols_idx_numeric = if self.options_write.should_infer_numeric_cols {
            select_numeric_column_indices_from_arrow_schema(schema)
        } else {
            vec![]
        };
        let cols_idx_integer_inferred = if self.options_write.should_infer_integer_cols {
            select_integer_column_indices_from_arrow_schema(schema, &cols_idx_numeric)
        } else {
            vec![]
        };
        let cols_idx_integer_specified =
            select_sorted_indices_from_refs(&col_names_ref, options.cols_integer.as_deref())?;
        let cols_idx_decimal_specified =
            select_sorted_indices_from_refs(&col_names_ref, options.cols_decimal.as_deref())?;
        let cols_idx_integer = if cols_idx_integer_specified.is_empty() {
            cols_idx_integer_inferred
        } else {
            cols_idx_integer_specified
        };

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
                options_write: &self.options_write,
            });
            let data_formats_by_col = column_format_plan
                .fmts_by_col
                .iter()
                .map(create_rust_xlsx_format)
                .collect::<Vec<_>>();
            let fmt_scientific_patch = self
                .fmt_scientific
                .merge(&self.options_write.base_format_patch);
            let fmt_scientific = create_rust_xlsx_format(&fmt_scientific_patch);
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

    fn write_sheet(
        &mut self,
        body: &DataFrame,
        sheet_name: &str,
        header: Option<&DataFrame>,
        options: &XlsxSheetWriteOptions,
    ) -> Result<(), String> {
        validate_policy_autofit(&options.policy_autofit)?;
        validate_policy_scientific(&options.policy_scientific)?;

        let should_keep_missing_values = options
            .should_keep_missing_values
            .unwrap_or(self.options_write.should_keep_missing_values);
        let value_policy = self.options_write.value_policy.clone();

        let col_names: Vec<&str> = body.get_column_names_str();
        validate_unique_columns(&col_names)?;

        let width_body = col_names.len();
        let height_body = body.height();

        let mut header_grid = vec![
            col_names
                .iter()
                .map(|&_val| _val.to_string())
                .collect::<Vec<String>>(),
        ];
        if let Some(df_header_custom) = header {
            let header_cols: Vec<&str> = df_header_custom.get_column_names_str();
            validate_unique_columns(&header_cols)?;

            let header_height = df_header_custom.height();
            if header_height == 0 {
                return Err("header must have >= 1 row (0-row header is not allowed).".to_string());
            }
            let header_width = df_header_custom.width();
            if header_width != width_body {
                return Err("header.width must equal body.width.".to_string());
            }

            header_grid = extract_string_grid_from_dataframe(df_header_custom)?;
        }

        let cols_idx_numeric = if self.options_write.should_infer_numeric_cols {
            select_numeric_column_indices(body)
        } else {
            vec![]
        };

        let cols_idx_integer_inferred = if self.options_write.should_infer_integer_cols {
            select_integer_column_indices(body, &cols_idx_numeric)
        } else {
            vec![]
        };

        let cols_idx_integer_specified =
            select_sorted_indices_from_refs(&col_names, options.cols_integer.as_deref())?;
        let cols_idx_decimal_specified =
            select_sorted_indices_from_refs(&col_names, options.cols_decimal.as_deref())?;

        let cols_idx_integer = if cols_idx_integer_specified.is_empty() {
            cols_idx_integer_inferred
        } else {
            cols_idx_integer_specified
        };
        let header_row_count = header_grid.len();

        let mut report = XlsxReport {
            sheets: vec![],
            warnings: vec![],
        };

        let sheet_slices = plan_sheet_slices(
            height_body,
            width_body,
            header_row_count,
            &sanitize_sheet_name(sheet_name, "_"),
            &mut report,
        )?;

        let num_frozen_rows = options.num_frozen_rows.unwrap_or(header_row_count);

        for _sheet_slice in sheet_slices {
            let sheet_slice = _sheet_slice;
            let sheet_name_unique = self.ensure_unique_sheet_name(&sheet_slice.sheet_name);
            let worksheet = self.workbook.add_worksheet();
            worksheet
                .set_name(&sheet_name_unique)
                .map_err(format_xlsx_error_text)?;

            let cols_idx_numeric_slice = calculate_slice_indices(
                &cols_idx_numeric,
                sheet_slice.col_start_inclusive,
                sheet_slice.col_end_exclusive,
            );
            let cols_idx_integer_slice = calculate_slice_indices(
                &cols_idx_integer,
                sheet_slice.col_start_inclusive,
                sheet_slice.col_end_exclusive,
            );
            let cols_idx_decimal_slice = calculate_slice_indices(
                &cols_idx_decimal_specified,
                sheet_slice.col_start_inclusive,
                sheet_slice.col_end_exclusive,
            );
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
                options_write: &self.options_write,
            });

            let data_formats_by_col: Vec<Format> = column_format_plan
                .fmts_by_col
                .iter()
                .map(create_rust_xlsx_format)
                .collect();
            let fmt_scientific_patch = self
                .fmt_scientific
                .merge(&self.options_write.base_format_patch);
            let fmt_scientific = create_rust_xlsx_format(&fmt_scientific_patch);
            let fmt_header = create_rust_xlsx_format(&self.fmt_header);

            let header_grid_slice = header_grid
                .iter()
                .map(|row| {
                    row[sheet_slice.col_start_inclusive..sheet_slice.col_end_exclusive].to_vec()
                })
                .collect::<Vec<_>>();

            let mut header_widths_by_col = vec![0usize; data_formats_by_col.len()];
            let mut body_widths_by_col = vec![0usize; data_formats_by_col.len()];

            let should_autofit_columns = !matches!(options.policy_autofit.mode, AutofitMode::None);

            if should_autofit_columns && !data_formats_by_col.is_empty() {
                for _col_idx in 0..data_formats_by_col.len() {
                    let col_idx = _col_idx;
                    for _row in &header_grid_slice {
                        let row = _row;
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
                                &value_policy,
                            ),
                        );
                    }
                }
            }

            write_header(
                worksheet,
                header_grid_slice,
                options.should_merge_header,
                &fmt_header,
            )?;

            worksheet
                .set_freeze_panes(
                    cast_row_num(num_frozen_rows)?,
                    cast_col_num(options.num_frozen_cols)?,
                )
                .map_err(format_xlsx_error_text)?;

            let numeric_cols_idx: BTreeSet<usize> =
                cols_idx_numeric_slice.iter().copied().collect();
            let integer_cols_idx: BTreeSet<usize> =
                cols_idx_integer_slice.iter().copied().collect();
            let decimal_cols_idx: BTreeSet<usize> =
                cols_idx_decimal_slice.iter().copied().collect();
            let is_decimal_explicit = !decimal_cols_idx.is_empty();

            let mut cols_slice = Vec::with_capacity(data_formats_by_col.len());
            let rows_data_in_sheet =
                sheet_slice.row_end_exclusive - sheet_slice.row_start_inclusive;
            for _col_idx_abs in sheet_slice.col_start_inclusive..sheet_slice.col_end_exclusive {
                let col_idx_abs = _col_idx_abs;
                cols_slice.push(
                    body.get_columns()[col_idx_abs]
                        .slice(sheet_slice.row_start_inclusive as i64, rows_data_in_sheet),
                );
            }
            let rows_chunk = calculate_row_chunk_size(
                data_formats_by_col.len(),
                &self.options_write.row_chunk_policy,
            );
            if rows_chunk == 0 {
                return Err("row_chunk_policy resolved to 0 rows; expected >= 1.".to_string());
            }
            let row_chunks = generate_row_chunks(rows_data_in_sheet, rows_chunk);

            let mut rows_seen_for_autofit = 0usize;
            for _row_chunk in row_chunks {
                let (row_chunk_start, row_chunk_len) = _row_chunk;
                let row_chunk_end = row_chunk_start + row_chunk_len;
                for _row_local in row_chunk_start..row_chunk_end {
                    let row_local = _row_local;
                    for _col in cols_slice.iter().enumerate() {
                        let (col_idx, col) = _col;
                        let is_numeric_col = numeric_cols_idx.contains(&col_idx);
                        let is_integer_col = integer_cols_idx.contains(&col_idx);
                        let is_decimal_specified = decimal_cols_idx.contains(&col_idx);
                        let is_scientific_candidate = is_scientific_candidate_col(
                            &options.policy_scientific,
                            is_integer_col,
                            is_decimal_explicit,
                            is_decimal_specified,
                        );

                        let value_raw = convert_any_value_to_cell_value(
                            col.get(row_local)
                                .map_err(|err| format!("Failed to access cell value: {err}"))?,
                        );
                        let value = convert_cell_value(
                            &value_raw,
                            is_numeric_col,
                            is_integer_col,
                            should_keep_missing_values,
                            &value_policy,
                        );

                        if should_autofit_columns
                            && (options.policy_autofit.height_body_inferred_max.is_none()
                                || rows_seen_for_autofit
                                    < options.policy_autofit.height_body_inferred_max.unwrap_or(0))
                        {
                            body_widths_by_col[col_idx] = usize::max(
                                body_widths_by_col[col_idx],
                                estimate_width_len(
                                    &value,
                                    is_numeric_col,
                                    is_integer_col,
                                    is_scientific_candidate,
                                    &options.policy_scientific,
                                    should_keep_missing_values,
                                    &value_policy,
                                ),
                            );
                        }

                        let should_use_scientific = should_use_scientific_value(
                            &value,
                            is_numeric_col,
                            is_scientific_candidate,
                            &options.policy_scientific,
                        );
                        let fmt_cell = if should_use_scientific {
                            &fmt_scientific
                        } else {
                            &data_formats_by_col[col_idx]
                        };

                        write_cell_with_format(
                            worksheet,
                            header_row_count + row_local,
                            col_idx,
                            &value,
                            fmt_cell,
                        )?;
                    }

                    if should_autofit_columns
                        && (options.policy_autofit.height_body_inferred_max.is_none()
                            || rows_seen_for_autofit
                                < options.policy_autofit.height_body_inferred_max.unwrap_or(0))
                    {
                        rows_seen_for_autofit += 1;
                    }
                }
            }

            if should_autofit_columns && !data_formats_by_col.is_empty() {
                let width_min = usize::max(1, options.policy_autofit.width_cell_min);
                let width_max = usize::min(
                    255,
                    usize::max(width_min, options.policy_autofit.width_cell_max),
                );
                let width_padding = options.policy_autofit.width_cell_padding;

                for _col_idx in 0..data_formats_by_col.len() {
                    let col_idx = _col_idx;
                    let width_recorded = match options.policy_autofit.mode {
                        AutofitMode::Header => header_widths_by_col[col_idx],
                        AutofitMode::Body => body_widths_by_col[col_idx],
                        AutofitMode::All => {
                            usize::max(header_widths_by_col[col_idx], body_widths_by_col[col_idx])
                        }
                        AutofitMode::None => header_widths_by_col[col_idx],
                    };
                    let width_final = usize::min(
                        width_max,
                        usize::max(width_min, width_recorded + width_padding),
                    );
                    worksheet
                        .set_column_width(cast_col_num(col_idx)?, width_final as f64)
                        .map_err(format_xlsx_error_text)?;
                }
            }

            report.sheets.push(SheetSlice {
                sheet_name: sheet_name_unique,
                row_start_inclusive: sheet_slice.row_start_inclusive,
                row_end_exclusive: sheet_slice.row_end_exclusive,
                col_start_inclusive: sheet_slice.col_start_inclusive,
                col_end_exclusive: sheet_slice.col_end_exclusive,
            });
        }

        self.reports.push(report);
        Ok(())
    }

    fn ensure_unique_sheet_name(&mut self, name: &str) -> String {
        if !self.existing_sheet_names.contains(name) {
            self.existing_sheet_names.insert(name.to_string());
            return name.to_string();
        }

        let base_name: String = name
            .chars()
            .take(usize::max(1, LEN_SHEET_NAME_MAX - 3))
            .collect();

        let mut idx = 2usize;
        loop {
            let candidate: String = format!("{base_name}__{idx}")
                .chars()
                .take(LEN_SHEET_NAME_MAX)
                .collect();
            if !self.existing_sheet_names.contains(&candidate) {
                self.existing_sheet_names.insert(candidate.clone());
                return candidate;
            }
            idx += 1;
        }
    }
}

impl<'a> XlsxSheetPlanBuilder<'a> {
    fn new(
        sheet_name: &'a str,
        header_grid_custom: Option<Vec<Vec<String>>>,
        options: &'a XlsxSheetWriteOptions,
        options_write: &'a XlsxWriteOptions,
    ) -> Self {
        let should_keep_missing_values = options
            .should_keep_missing_values
            .unwrap_or(options_write.should_keep_missing_values);

        Self {
            sheet_name,
            header_grid_custom,
            options,
            options_write,
            value_policy: options_write.value_policy.clone(),
            col_names: None,
            height_body: 0,
            width_body: 0,
            cols_idx_numeric: vec![],
            cols_idx_integer: vec![],
            cols_idx_decimal_specified: vec![],
            header_widths_by_col: vec![],
            body_widths_by_col: vec![],
            rows_seen_for_autofit: 0,
            should_keep_missing_values,
        }
    }

    fn scan_batch(&mut self, batch: XlsxRecordBatch) -> Result<(), String> {
        let df_batch = dataframe_from_record_batch(batch)?;
        self.ensure_initialized(&df_batch)?;

        let should_scan_body_width = matches!(
            self.options.policy_autofit.mode,
            AutofitMode::Body | AutofitMode::All
        );
        if should_scan_body_width && df_batch.width() > 0 {
            self.scan_body_widths(&df_batch)?;
        }
        self.height_body += df_batch.height();
        Ok(())
    }

    fn ensure_initialized(&mut self, df_batch: &DataFrame) -> Result<(), String> {
        let batch_col_names = df_batch
            .get_column_names_str()
            .iter()
            .map(|val| (*val).to_string())
            .collect::<Vec<_>>();

        if let Some(col_names) = &self.col_names {
            if col_names != &batch_col_names {
                return Err("All record batches must have identical column names.".to_string());
            }
            return Ok(());
        }

        let col_names_ref = batch_col_names
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        validate_unique_columns(&col_names_ref)?;
        self.width_body = batch_col_names.len();

        let header_grid = match &self.header_grid_custom {
            Some(header_grid) => {
                if header_grid.is_empty() {
                    return Err(
                        "header must have >= 1 row (0-row header is not allowed).".to_string()
                    );
                }
                if header_grid.iter().any(|row| row.len() != self.width_body) {
                    return Err("header.width must equal body.width.".to_string());
                }
                header_grid.clone()
            }
            None => vec![batch_col_names.clone()],
        };

        self.cols_idx_numeric = if self.options_write.should_infer_numeric_cols {
            select_numeric_column_indices(df_batch)
        } else {
            vec![]
        };

        let cols_idx_integer_inferred = if self.options_write.should_infer_integer_cols {
            select_integer_column_indices(df_batch, &self.cols_idx_numeric)
        } else {
            vec![]
        };

        let cols_idx_integer_specified =
            select_sorted_indices_from_refs(&col_names_ref, self.options.cols_integer.as_deref())?;
        self.cols_idx_decimal_specified =
            select_sorted_indices_from_refs(&col_names_ref, self.options.cols_decimal.as_deref())?;

        let rows_chunk =
            calculate_row_chunk_size(self.width_body, &self.options_write.row_chunk_policy);
        if rows_chunk == 0 {
            return Err("row_chunk_policy resolved to 0 rows; expected >= 1.".to_string());
        }

        self.cols_idx_integer = if cols_idx_integer_specified.is_empty() {
            cols_idx_integer_inferred
        } else {
            cols_idx_integer_specified
        };

        self.header_widths_by_col = vec![0usize; self.width_body];
        self.body_widths_by_col = vec![0usize; self.width_body];
        if !matches!(self.options.policy_autofit.mode, AutofitMode::None) {
            for col_idx in 0..self.width_body {
                for row in &header_grid {
                    let value = &row[col_idx];
                    if value.is_empty() {
                        continue;
                    }
                    self.header_widths_by_col[col_idx] = usize::max(
                        self.header_widths_by_col[col_idx],
                        estimate_width_len(
                            &CellValue::String(value.clone()),
                            false,
                            false,
                            false,
                            &self.options.policy_scientific,
                            self.should_keep_missing_values,
                            &self.value_policy,
                        ),
                    );
                }
            }
        }

        self.header_grid_custom = Some(header_grid);
        self.col_names = Some(batch_col_names);
        Ok(())
    }

    fn scan_body_widths(&mut self, df_batch: &DataFrame) -> Result<(), String> {
        let Some(max_rows) = self.options.policy_autofit.height_body_inferred_max else {
            return self.scan_body_width_rows(df_batch, df_batch.height());
        };
        if self.rows_seen_for_autofit >= max_rows {
            return Ok(());
        }
        let remaining = max_rows - self.rows_seen_for_autofit;
        let rows_to_scan = usize::min(remaining, df_batch.height());
        self.scan_body_width_rows(df_batch, rows_to_scan)
    }

    fn scan_body_width_rows(
        &mut self,
        df_batch: &DataFrame,
        rows_to_scan: usize,
    ) -> Result<(), String> {
        let numeric_cols_idx: BTreeSet<usize> = self.cols_idx_numeric.iter().copied().collect();
        let integer_cols_idx: BTreeSet<usize> = self.cols_idx_integer.iter().copied().collect();
        let decimal_cols_idx: BTreeSet<usize> =
            self.cols_idx_decimal_specified.iter().copied().collect();
        let is_decimal_explicit = !decimal_cols_idx.is_empty();

        for row_local in 0..rows_to_scan {
            for (col_idx, col) in df_batch.get_columns().iter().enumerate() {
                let is_numeric_col = numeric_cols_idx.contains(&col_idx);
                let is_integer_col = integer_cols_idx.contains(&col_idx);
                let is_decimal_specified = decimal_cols_idx.contains(&col_idx);
                let is_scientific_candidate = is_scientific_candidate_col(
                    &self.options.policy_scientific,
                    is_integer_col,
                    is_decimal_explicit,
                    is_decimal_specified,
                );
                let value_raw = convert_any_value_to_cell_value(
                    col.get(row_local)
                        .map_err(|err| format!("Failed to access cell value: {err}"))?,
                );
                let value = convert_cell_value(
                    &value_raw,
                    is_numeric_col,
                    is_integer_col,
                    self.should_keep_missing_values,
                    &self.value_policy,
                );
                self.body_widths_by_col[col_idx] = usize::max(
                    self.body_widths_by_col[col_idx],
                    estimate_width_len(
                        &value,
                        is_numeric_col,
                        is_integer_col,
                        is_scientific_candidate,
                        &self.options.policy_scientific,
                        self.should_keep_missing_values,
                        &self.value_policy,
                    ),
                );
            }
            self.rows_seen_for_autofit += 1;
        }
        Ok(())
    }

    fn finish(self) -> Result<XlsxSheetPlan, String> {
        let col_names = self.col_names.ok_or_else(|| {
            "Cannot write sheet from an empty batch stream with unknown schema.".to_string()
        })?;
        let header_grid = self
            .header_grid_custom
            .ok_or_else(|| "Missing resolved header grid.".to_string())?;
        let header_row_count = header_grid.len();
        let mut report = XlsxReport {
            sheets: vec![],
            warnings: vec![],
        };
        let sheet_slices = plan_sheet_slices(
            self.height_body,
            self.width_body,
            header_row_count,
            &sanitize_sheet_name(self.sheet_name, "_"),
            &mut report,
        )?;

        Ok(XlsxSheetPlan {
            col_names,
            height_body: self.height_body,
            header_grid,
            cols_idx_numeric: self.cols_idx_numeric,
            cols_idx_integer: self.cols_idx_integer,
            cols_idx_decimal_specified: self.cols_idx_decimal_specified,
            header_widths_by_col: self.header_widths_by_col,
            body_widths_by_col: self.body_widths_by_col,
            sheet_slices,
            num_frozen_rows: self.options.num_frozen_rows.unwrap_or(header_row_count),
            should_keep_missing_values: self.should_keep_missing_values,
        })
    }
}

fn dataframe_from_record_batch(batch: XlsxRecordBatch) -> Result<DataFrame, String> {
    let schema_arrow = batch.schema().clone();
    let mut df = DataFrame::empty_with_arrow_schema(&schema_arrow);
    df.try_extend(std::iter::once(batch))
        .map_err(|err| format!("Failed to convert Arrow record batch to DataFrame: {err}"))?;
    Ok(df)
}

fn apply_column_widths(
    worksheet: &mut Worksheet,
    policy_autofit: &AutofitPolicy,
    header_widths_by_col: &[usize],
    body_widths_by_col: &[usize],
) -> Result<(), String> {
    if matches!(policy_autofit.mode, AutofitMode::None) || header_widths_by_col.is_empty() {
        return Ok(());
    }

    let width_min = usize::max(1, policy_autofit.width_cell_min);
    let width_max = usize::min(255, usize::max(width_min, policy_autofit.width_cell_max));
    let width_padding = policy_autofit.width_cell_padding;

    for col_idx in 0..header_widths_by_col.len() {
        let width_recorded = match policy_autofit.mode {
            AutofitMode::Header => header_widths_by_col[col_idx],
            AutofitMode::Body => body_widths_by_col[col_idx],
            AutofitMode::All => {
                usize::max(header_widths_by_col[col_idx], body_widths_by_col[col_idx])
            }
            AutofitMode::None => header_widths_by_col[col_idx],
        };
        let width_final = usize::min(
            width_max,
            usize::max(width_min, width_recorded + width_padding),
        );
        worksheet
            .set_column_width(cast_col_num(col_idx)?, width_final as f64)
            .map_err(format_xlsx_error_text)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_record_batch_to_runtime_sheet(
    workbook: &mut Workbook,
    runtime: &XlsxSheetRuntime,
    df_batch: &DataFrame,
    row_offset: usize,
    header_row_count: usize,
    should_keep_missing_values: bool,
    value_policy: &XlsxValuePolicy,
    policy_scientific: &ScientificPolicy,
) -> Result<(), String> {
    let batch_start = row_offset;
    let batch_end = row_offset + df_batch.height();
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
            let col = &df_batch.get_columns()[col_abs];
            let is_numeric_col = runtime.numeric_cols_idx.contains(&col_idx);
            let is_integer_col = runtime.integer_cols_idx.contains(&col_idx);
            let is_decimal_specified = runtime.decimal_cols_idx.contains(&col_idx);
            let is_scientific_candidate = is_scientific_candidate_col(
                policy_scientific,
                is_integer_col,
                runtime.is_decimal_explicit,
                is_decimal_specified,
            );
            let value_raw = convert_any_value_to_cell_value(
                col.get(row_local_in_batch)
                    .map_err(|err| format!("Failed to access cell value: {err}"))?,
            );
            let value = convert_cell_value(
                &value_raw,
                is_numeric_col,
                is_integer_col,
                should_keep_missing_values,
                value_policy,
            );
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
            let value_raw = convert_arrow_value_to_cell_value(col.as_ref(), row_local_in_batch)?;
            let value = convert_cell_value(
                &value_raw,
                is_numeric_col,
                is_integer_col,
                should_keep_missing_values,
                value_policy,
            );
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

/// Estimate displayed width units for one normalized cell value.
///
/// Used by autofit inference logic.
fn estimate_width_len(
    value: &CellValue,
    is_numeric_col: bool,
    is_integer_col: bool,
    is_scientific_candidate: bool,
    policy_scientific: &ScientificPolicy,
    should_keep_missing_values: bool,
    value_policy: &XlsxValuePolicy,
) -> usize {
    match value {
        CellValue::None => {
            if should_keep_missing_values {
                value_policy.missing_value_str.len()
            } else {
                0
            }
        }
        CellValue::String(s) => {
            if s.is_empty() {
                return 0;
            }
            if !is_numeric_col {
                return estimate_unicode_string_width(s);
            }
            if is_integer_col && let Ok(val) = s.parse::<i64>() {
                return val.to_string().len();
            }
            estimate_unicode_string_width(s)
        }
        CellValue::Number(n) => {
            if !is_numeric_col {
                return estimate_unicode_string_width(&n.to_string());
            }
            if should_use_scientific_value(
                value,
                is_numeric_col,
                is_scientific_candidate,
                policy_scientific,
            ) {
                return format!("{n:.2E}").len();
            }
            if is_integer_col {
                return (*n as i64).to_string().len();
            }
            format!("{n:.4}").len()
        }
    }
}

fn estimate_unicode_string_width(s: &str) -> usize {
    let ascii_count = s.chars().filter(|chr| chr.is_ascii()).count();
    let non_ascii_count = s.chars().count().saturating_sub(ascii_count);
    ascii_count + (non_ascii_count as f64 * 1.6).round() as usize
}

/// Build per-column base/final format plans for current sheet slice.
fn plan_column_formats(options: ColumnFormatPlanOptions<'_>) -> ColumnFormatPlan {
    let ColumnFormatPlanOptions {
        width_data,
        cols_idx_numeric,
        cols_idx_integer,
        cols_idx_decimal,
        cols_fmt_overrides,
        fmt_text,
        fmt_integer,
        fmt_decimal,
        options_write,
    } = options;

    let numeric_cols_idx: BTreeSet<usize> = cols_idx_numeric.iter().copied().collect();
    let integer_cols_idx: BTreeSet<usize> = cols_idx_integer.iter().copied().collect();
    let decimal_cols_idx: Option<BTreeSet<usize>> =
        cols_idx_decimal.map(|vals| vals.iter().copied().collect());
    let mut fmts_base_by_col = Vec::with_capacity(width_data);
    let mut fmts_by_col = Vec::with_capacity(width_data);

    for _col_idx in 0..width_data {
        let col_idx = _col_idx;
        let mut fmt_base = if integer_cols_idx.contains(&col_idx) {
            fmt_integer.clone()
        } else if decimal_cols_idx
            .as_ref()
            .map_or(numeric_cols_idx.contains(&col_idx), |indices| {
                indices.contains(&col_idx)
            })
        {
            fmt_decimal.clone()
        } else {
            fmt_text.clone()
        };

        fmt_base = fmt_base.merge(&options_write.base_format_patch);

        let fmt_final = if let Some(fmt_override) = cols_fmt_overrides.get(&col_idx) {
            fmt_base.merge(fmt_override)
        } else {
            fmt_base.clone()
        };

        fmts_base_by_col.push(fmt_base);
        fmts_by_col.push(fmt_final);
    }

    ColumnFormatPlan {
        fmts_by_col,
        fmts_base_by_col,
    }
}

fn read_dataframe_from_ipc_bytes(ipc_df: &[u8]) -> Result<DataFrame, String> {
    IpcReader::new(Cursor::new(ipc_df))
        .finish()
        .map_err(|err| format!("Failed to read IPC DataFrame bytes: {err}"))
}

fn validate_policy_autofit(policy_autofit: &AutofitPolicy) -> Result<(), String> {
    if policy_autofit.width_cell_min == 0 {
        return Err("policy_autofit.width_cell_min must be >= 1.".to_string());
    }
    if policy_autofit.width_cell_max < policy_autofit.width_cell_min {
        return Err(
            "policy_autofit.width_cell_max must be >= policy_autofit.width_cell_min.".to_string(),
        );
    }
    Ok(())
}

fn validate_policy_scientific(policy_scientific: &ScientificPolicy) -> Result<(), String> {
    if policy_scientific.thr_min < 0.0 {
        return Err("policy_scientific.thr_min must be >= 0.".to_string());
    }
    if policy_scientific.thr_max <= 0.0 {
        return Err("policy_scientific.thr_max must be > 0.".to_string());
    }
    if policy_scientific.thr_min > policy_scientific.thr_max {
        return Err("policy_scientific.thr_min must be <= policy_scientific.thr_max.".to_string());
    }
    Ok(())
}

fn is_scientific_candidate_col(
    policy_scientific: &ScientificPolicy,
    is_integer_col: bool,
    is_decimal_explicit: bool,
    is_decimal_specified: bool,
) -> bool {
    match policy_scientific.scope {
        ScientificScope::None => false,
        ScientificScope::Decimal => {
            if is_integer_col {
                false
            } else if is_decimal_explicit {
                is_decimal_specified
            } else {
                true
            }
        }
        ScientificScope::Integer => is_integer_col,
        ScientificScope::All => true,
    }
}

fn should_use_scientific_value(
    value: &CellValue,
    is_numeric_col: bool,
    is_scientific_candidate: bool,
    policy_scientific: &ScientificPolicy,
) -> bool {
    if !is_numeric_col || !is_scientific_candidate {
        return false;
    }
    let CellValue::Number(value_num) = value else {
        return false;
    };
    if !value_num.is_finite() {
        return false;
    }
    let value_abs = value_num.abs();
    value_abs >= policy_scientific.thr_max
        || (value_abs > 0.0 && value_abs < policy_scientific.thr_min)
}

fn select_numeric_column_indices(df: &DataFrame) -> Vec<usize> {
    df.get_columns()
        .iter()
        .enumerate()
        .filter_map(|(idx, col)| {
            if col.dtype().is_numeric() {
                Some(idx)
            } else {
                None
            }
        })
        .collect()
}

fn select_integer_column_indices(df: &DataFrame, cols_idx_numeric: &[usize]) -> Vec<usize> {
    cols_idx_numeric
        .iter()
        .copied()
        .filter(|idx| df.get_columns()[*idx].dtype().is_integer())
        .collect()
}

fn select_numeric_column_indices_from_arrow_schema(schema: &ArrowSchema) -> Vec<usize> {
    schema
        .iter_values()
        .enumerate()
        .filter_map(|(idx, field)| {
            if is_arrow_numeric_dtype(field.dtype()) {
                Some(idx)
            } else {
                None
            }
        })
        .collect()
}

fn select_integer_column_indices_from_arrow_schema(
    schema: &ArrowSchema,
    cols_idx_numeric: &[usize],
) -> Vec<usize> {
    cols_idx_numeric
        .iter()
        .copied()
        .filter(|idx| {
            schema
                .get_at_index(*idx)
                .map(|(_, field)| is_arrow_integer_dtype(field.dtype()))
                .unwrap_or(false)
        })
        .collect()
}

fn is_arrow_numeric_dtype(dtype: &ArrowDataType) -> bool {
    matches!(
        dtype,
        ArrowDataType::Int8
            | ArrowDataType::Int16
            | ArrowDataType::Int32
            | ArrowDataType::Int64
            | ArrowDataType::Int128
            | ArrowDataType::UInt8
            | ArrowDataType::UInt16
            | ArrowDataType::UInt32
            | ArrowDataType::UInt64
            | ArrowDataType::Float16
            | ArrowDataType::Float32
            | ArrowDataType::Float64
            | ArrowDataType::Decimal(_, _)
            | ArrowDataType::Decimal32(_, _)
            | ArrowDataType::Decimal64(_, _)
            | ArrowDataType::Decimal256(_, _)
    )
}

fn is_arrow_integer_dtype(dtype: &ArrowDataType) -> bool {
    matches!(
        dtype,
        ArrowDataType::Int8
            | ArrowDataType::Int16
            | ArrowDataType::Int32
            | ArrowDataType::Int64
            | ArrowDataType::Int128
            | ArrowDataType::UInt8
            | ArrowDataType::UInt16
            | ArrowDataType::UInt32
            | ArrowDataType::UInt64
    )
}

fn extract_string_grid_from_dataframe(df: &DataFrame) -> Result<Vec<Vec<String>>, String> {
    let height = df.height();
    let width = df.width();
    let cols = df.get_columns();

    let mut grid = vec![vec![String::new(); width]; height];
    for (_row_index, _row_values) in grid.iter_mut().enumerate() {
        for (_col_index, _cell_value) in _row_values.iter_mut().enumerate() {
            let value = cols[_col_index]
                .get(_row_index)
                .map_err(|err| format!("Failed to read header cell value: {err}"))?;
            *_cell_value = format_header_text_from_any_value(value);
        }
    }

    Ok(grid)
}

fn format_header_text_from_any_value(value: AnyValue<'_>) -> String {
    match value {
        AnyValue::Null => String::new(),
        // Keep raw string payload for header cells; AnyValue::to_string() wraps
        // Utf8 values in quotes, which leaks into XLSX header text.
        AnyValue::String(val) => val.to_string(),
        AnyValue::StringOwned(val) => val.to_string(),
        _ => value.to_string(),
    }
}

fn convert_any_value_to_cell_value(value: AnyValue<'_>) -> CellValue {
    match value {
        AnyValue::Null => CellValue::None,
        AnyValue::String(val) => CellValue::String(val.to_string()),
        AnyValue::StringOwned(val) => CellValue::String(val.to_string()),
        AnyValue::Boolean(val) => CellValue::String(if val { "True" } else { "False" }.to_string()),
        AnyValue::UInt8(val) => CellValue::Number(val as f64),
        AnyValue::UInt16(val) => CellValue::Number(val as f64),
        AnyValue::UInt32(val) => CellValue::Number(val as f64),
        AnyValue::UInt64(val) => CellValue::Number(val as f64),
        AnyValue::Int8(val) => CellValue::Number(val as f64),
        AnyValue::Int16(val) => CellValue::Number(val as f64),
        AnyValue::Int32(val) => CellValue::Number(val as f64),
        AnyValue::Int64(val) => CellValue::Number(val as f64),
        AnyValue::Int128(val) => CellValue::Number(val as f64),
        AnyValue::Float32(val) => CellValue::Number(val as f64),
        AnyValue::Float64(val) => CellValue::Number(val),
        _ => CellValue::String(value.to_string()),
    }
}

fn convert_arrow_value_to_cell_value(
    array: &dyn ArrowArray,
    row_idx: usize,
) -> Result<CellValue, String> {
    if array.is_null(row_idx) {
        return Ok(CellValue::None);
    }

    macro_rules! primitive_number {
        ($array_ty:ty) => {{
            let arr = array.as_any().downcast_ref::<$array_ty>().ok_or_else(|| {
                format!(
                    "Failed to downcast Arrow array with dtype {:?}",
                    array.dtype()
                )
            })?;
            Ok(CellValue::Number(arr.value(row_idx) as f64))
        }};
    }

    match array.dtype() {
        ArrowDataType::Null => Ok(CellValue::None),
        ArrowDataType::Boolean => {
            let arr = array
                .as_any()
                .downcast_ref::<BooleanArray>()
                .ok_or_else(|| {
                    format!(
                        "Failed to downcast Arrow array with dtype {:?}",
                        array.dtype()
                    )
                })?;
            Ok(CellValue::String(
                if arr.value(row_idx) { "True" } else { "False" }.to_string(),
            ))
        }
        ArrowDataType::Int8 => primitive_number!(PrimitiveArray<i8>),
        ArrowDataType::Int16 => primitive_number!(PrimitiveArray<i16>),
        ArrowDataType::Int32 => primitive_number!(PrimitiveArray<i32>),
        ArrowDataType::Int64 => primitive_number!(PrimitiveArray<i64>),
        ArrowDataType::Int128 => primitive_number!(PrimitiveArray<i128>),
        ArrowDataType::UInt8 => primitive_number!(PrimitiveArray<u8>),
        ArrowDataType::UInt16 => primitive_number!(PrimitiveArray<u16>),
        ArrowDataType::UInt32 => primitive_number!(PrimitiveArray<u32>),
        ArrowDataType::UInt64 => primitive_number!(PrimitiveArray<u64>),
        ArrowDataType::Float32 => primitive_number!(PrimitiveArray<f32>),
        ArrowDataType::Float64 => primitive_number!(PrimitiveArray<f64>),
        ArrowDataType::Utf8 => {
            let arr = array
                .as_any()
                .downcast_ref::<Utf8Array<i32>>()
                .ok_or_else(|| {
                    format!(
                        "Failed to downcast Arrow array with dtype {:?}",
                        array.dtype()
                    )
                })?;
            Ok(CellValue::String(arr.value(row_idx).to_string()))
        }
        ArrowDataType::LargeUtf8 => {
            let arr = array
                .as_any()
                .downcast_ref::<Utf8Array<i64>>()
                .ok_or_else(|| {
                    format!(
                        "Failed to downcast Arrow array with dtype {:?}",
                        array.dtype()
                    )
                })?;
            Ok(CellValue::String(arr.value(row_idx).to_string()))
        }
        ArrowDataType::Utf8View => {
            let arr = array
                .as_any()
                .downcast_ref::<Utf8ViewArray>()
                .ok_or_else(|| {
                    format!(
                        "Failed to downcast Arrow array with dtype {:?}",
                        array.dtype()
                    )
                })?;
            Ok(CellValue::String(arr.value(row_idx).to_string()))
        }
        dtype => Ok(CellValue::String(format!("{dtype:?}"))),
    }
}

fn calculate_slice_indices(
    indices: &[usize],
    col_start_inclusive: usize,
    col_end_exclusive: usize,
) -> Vec<usize> {
    indices
        .iter()
        .filter_map(|idx| {
            if *idx >= col_start_inclusive && *idx < col_end_exclusive {
                Some(*idx - col_start_inclusive)
            } else {
                None
            }
        })
        .collect()
}

fn create_sheet_identifier_local(sheet_name: &str, part_idx: usize) -> String {
    let suffix = format!("__{part_idx}");
    let prefix_len = LEN_SHEET_NAME_MAX.saturating_sub(suffix.chars().count());
    let prefix = sheet_name.chars().take(prefix_len).collect::<String>();
    format!("{prefix}{suffix}")
}

fn write_header_cell(
    worksheet: &mut Worksheet,
    row_idx: usize,
    col_idx: usize,
    text: &str,
    fmt_header: &Format,
) -> Result<(), String> {
    if text.is_empty() {
        worksheet
            .write_blank(cast_row_num(row_idx)?, cast_col_num(col_idx)?, fmt_header)
            .map_err(format_xlsx_error_text)?;
    } else {
        worksheet
            .write_string_with_format(
                cast_row_num(row_idx)?,
                cast_col_num(col_idx)?,
                text,
                fmt_header,
            )
            .map_err(format_xlsx_error_text)?;
    }
    Ok(())
}

fn write_header(
    worksheet: &mut Worksheet,
    mut header_grid: Vec<Vec<String>>,
    should_merge: bool,
    fmt_header: &Format,
) -> Result<(), String> {
    if !should_merge {
        for (_row_idx, _row_values) in header_grid.iter().enumerate() {
            for (_col_idx, _cell_value) in _row_values.iter().enumerate() {
                write_header_cell(worksheet, _row_idx, _col_idx, _cell_value, fmt_header)?;
            }
        }
        return Ok(());
    }

    apply_vertical_run_text_blankout(&mut header_grid);
    let horizontal_merges_by_row = plan_horizontal_merges(&header_grid);
    let horizontal_merge_tracker = create_horizontal_merge_tracker(&horizontal_merges_by_row);

    for (_row_idx, _row_values) in header_grid.iter().enumerate() {
        for (_col_idx, _cell_value) in _row_values.iter().enumerate() {
            if horizontal_merge_tracker
                .get(&(_row_idx, _col_idx))
                .copied()
                .unwrap_or(false)
            {
                continue;
            }

            write_header_cell(worksheet, _row_idx, _col_idx, _cell_value, fmt_header)?;
        }

        if let Some(merges) = horizontal_merges_by_row.get(&_row_idx) {
            for _merge in merges {
                let merge = _merge;
                worksheet
                    .merge_range(
                        cast_row_num(_row_idx)?,
                        cast_col_num(merge.col_idx_start)?,
                        cast_row_num(_row_idx)?,
                        cast_col_num(merge.col_idx_end)?,
                        &merge.text,
                        fmt_header,
                    )
                    .map_err(format_xlsx_error_text)?;
            }
        }
    }

    Ok(())
}

fn write_cell_with_format(
    worksheet: &mut Worksheet,
    row_idx: usize,
    col_idx: usize,
    value: &CellValue,
    format: &Format,
) -> Result<(), String> {
    match value {
        CellValue::None => {
            worksheet
                .write_blank(cast_row_num(row_idx)?, cast_col_num(col_idx)?, format)
                .map_err(format_xlsx_error_text)?;
        }
        CellValue::String(val) => {
            worksheet
                .write_string_with_format(
                    cast_row_num(row_idx)?,
                    cast_col_num(col_idx)?,
                    val,
                    format,
                )
                .map_err(format_xlsx_error_text)?;
        }
        CellValue::Number(val) => {
            worksheet
                .write_number_with_format(
                    cast_row_num(row_idx)?,
                    cast_col_num(col_idx)?,
                    *val,
                    format,
                )
                .map_err(format_xlsx_error_text)?;
        }
    }
    Ok(())
}

fn create_rust_xlsx_format(spec: &CellFormatPatch) -> Format {
    let mut format = Format::new();

    if let Some(val) = &spec.font_name {
        format = format.set_font_name(val.clone());
    }
    if let Some(val) = spec.font_size {
        format = format.set_font_size(val as f64);
    }
    if spec.bold.unwrap_or(false) {
        format = format.set_bold();
    }
    if spec.italic.unwrap_or(false) {
        format = format.set_italic();
    }

    if let Some(val) = &spec.align
        && let Some(align) = parse_format_align(val)
    {
        format = format.set_align(align);
    }
    if let Some(val) = &spec.valign
        && let Some(align) = parse_format_align(val)
    {
        format = format.set_align(align);
    }

    if let Some(val) = &spec.num_format {
        format = format.set_num_format(val.clone());
    }
    if let Some(val) = &spec.bg_color {
        format = format.set_background_color(val.as_str());
    }
    if let Some(val) = &spec.font_color {
        format = format.set_font_color(val.as_str());
    }

    if let Some(val) = spec.border {
        format = format.set_border(parse_format_border(val));
    }
    if let Some(val) = spec.top {
        format = format.set_border_top(parse_format_border(val));
    }
    if let Some(val) = spec.bottom {
        format = format.set_border_bottom(parse_format_border(val));
    }
    if let Some(val) = spec.left {
        format = format.set_border_left(parse_format_border(val));
    }
    if let Some(val) = spec.right {
        format = format.set_border_right(parse_format_border(val));
    }

    if spec.text_wrap.unwrap_or(false) {
        format = format.set_text_wrap();
    }

    format
}

fn parse_format_border(border: i64) -> FormatBorder {
    match border {
        0 => FormatBorder::None,
        1 => FormatBorder::Thin,
        2 => FormatBorder::Medium,
        3 => FormatBorder::Dashed,
        4 => FormatBorder::Dotted,
        5 => FormatBorder::Thick,
        6 => FormatBorder::Double,
        7 => FormatBorder::Hair,
        8 => FormatBorder::MediumDashed,
        9 => FormatBorder::DashDot,
        10 => FormatBorder::MediumDashDot,
        11 => FormatBorder::DashDotDot,
        12 => FormatBorder::MediumDashDotDot,
        13 => FormatBorder::SlantDashDot,
        _ => FormatBorder::None,
    }
}

fn parse_format_align(align: &str) -> Option<FormatAlign> {
    let value = align.trim().to_ascii_lowercase();
    match value.as_str() {
        "general" => Some(FormatAlign::General),
        "left" => Some(FormatAlign::Left),
        "center" => Some(FormatAlign::Center),
        "right" => Some(FormatAlign::Right),
        "fill" => Some(FormatAlign::Fill),
        "justify" => Some(FormatAlign::Justify),
        "center_across" => Some(FormatAlign::CenterAcross),
        "distributed" => Some(FormatAlign::Distributed),
        "top" => Some(FormatAlign::Top),
        "bottom" => Some(FormatAlign::Bottom),
        "vcenter" | "vertical_center" => Some(FormatAlign::VerticalCenter),
        "vjustify" | "vertical_justify" => Some(FormatAlign::VerticalJustify),
        "vdistributed" | "vertical_distributed" => Some(FormatAlign::VerticalDistributed),
        _ => None,
    }
}

fn cast_row_num(value: usize) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| format!("row index overflow: {value}"))
}

fn cast_col_num(value: usize) -> Result<u16, String> {
    u16::try_from(value).map_err(|_| format!("column index overflow: {value}"))
}

fn format_xlsx_error_text(err: XlsxError) -> String {
    format!("xlsx write error: {err}")
}
