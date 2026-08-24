//! XLSX writer lifecycle and public Rust entrypoints.

mod plan;
mod render;
mod stream;
mod value;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use polars::prelude::DataFrame;
use rust_xlsxwriter::{Format, Workbook};

use crate::constant::{ColumnIdentifier, LEN_SHEET_NAME_MAX};
use crate::spec::{
    AutofitMode, AutofitPolicy, CellFormatPatch, CellValue, ColumnValuePlan, ScientificPolicy,
    SheetSlice, XlsxReport, XlsxWriteOptions,
};
use crate::util::{
    calculate_row_chunk_size, convert_cell_value, generate_row_chunks, plan_sheet_slices,
    sanitize_sheet_name, select_sorted_indices_from_refs, validate_unique_columns,
};
pub use plan::XlsxSheetPlan;
use plan::calculate_slice_indices;
use render::{
    ColumnFormatPlanOptions, cast_col_num, cast_row_num, create_rust_xlsx_format,
    format_xlsx_error_text, inferred_num_formats, plan_column_formats, plan_header_formats,
    plan_scientific_formats, slice_column_format_overrides, write_cell_with_format, write_header,
};
pub use stream::{XlsxRecordBatch, XlsxRecordBatchResult};
use value::{
    convert_any_value_to_cell_value, estimate_width_len, extract_string_grid_from_dataframe,
    is_scientific_candidate_col, read_dataframe_from_ipc_bytes, select_integer_column_indices,
    select_numeric_column_indices, should_use_scientific_value, validate_policy_autofit,
    validate_policy_scientific,
};

/// Per-sheet call options (aligned with Python `XlsxWriter.write_sheet` kwargs).
#[derive(Default, Debug, Clone)]
pub struct XlsxSheetWriteOptions {
    /// Optional per-row patches for a custom header; `None` inherits the writer header format.
    pub header_row_formats: Vec<Option<CellFormatPatch>>,
    /// Per-source-column body format patches keyed by zero-based logical index.
    pub column_formats: BTreeMap<usize, CellFormatPatch>,
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
    /// Ordered source-column dtype metadata from Python preflight.
    pub value_plans: Vec<ColumnValuePlan>,
}

/// Stateful workbook writer.
pub struct XlsxWriter {
    path_file_out: PathBuf,
    workbook: Workbook,
    fmt_text: CellFormatPatch,
    fmt_text_override: CellFormatPatch,
    fmt_integer: CellFormatPatch,
    fmt_integer_override: CellFormatPatch,
    fmt_decimal: CellFormatPatch,
    fmt_decimal_override: CellFormatPatch,
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
    ) -> Result<Self, String> {
        let mut workbook = Workbook::new();
        workbook.use_zip_large_file(options_write.should_use_zip64);
        if let Some(parent) = path_file_out.parent() {
            workbook
                .set_tempdir(parent)
                .map_err(format_xlsx_error_text)?;
        }

        let defaults = crate::constant::create_default_xlsx_formats();
        let fmt_text_default = defaults
            .get("text")
            .cloned()
            .ok_or_else(|| "Missing default format: text".to_string())?;
        let fmt_integer_default = defaults
            .get("integer")
            .cloned()
            .ok_or_else(|| "Missing default format: integer".to_string())?;
        let fmt_decimal_default = defaults
            .get("decimal")
            .cloned()
            .ok_or_else(|| "Missing default format: decimal".to_string())?;
        let fmt_scientific_default = defaults
            .get("scientific")
            .cloned()
            .ok_or_else(|| "Missing default format: scientific".to_string())?;
        let fmt_header_default = defaults
            .get("header")
            .cloned()
            .ok_or_else(|| "Missing default format: header".to_string())?;

        Ok(Self {
            path_file_out,
            workbook,
            fmt_text: fmt_text_default.merge(&fmt_text),
            fmt_text_override: fmt_text,
            fmt_integer: fmt_integer_default.merge(&fmt_integer),
            fmt_integer_override: fmt_integer,
            fmt_decimal: fmt_decimal_default.merge(&fmt_decimal),
            fmt_decimal_override: fmt_decimal,
            fmt_scientific: fmt_scientific_default.merge(&fmt_scientific),
            fmt_header: fmt_header_default.merge(&fmt_header),
            options_write,
            existing_sheet_names: BTreeSet::new(),
            reports: Vec::new(),
            is_closed: false,
        })
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

        let mut cols_idx_numeric = if self.options_write.should_infer_numeric_cols {
            select_numeric_column_indices(body)
        } else {
            vec![]
        };
        let cols_idx_integer_specified =
            select_sorted_indices_from_refs(&col_names, options.cols_integer.as_deref())?;
        let cols_idx_decimal_specified =
            select_sorted_indices_from_refs(&col_names, options.cols_decimal.as_deref())?;
        cols_idx_numeric.extend(cols_idx_integer_specified.iter().copied());
        cols_idx_numeric.extend(cols_idx_decimal_specified.iter().copied());
        cols_idx_numeric.sort_unstable();
        cols_idx_numeric.dedup();
        let mut cols_idx_integer = if self.options_write.should_infer_integer_cols {
            select_integer_column_indices(body, &cols_idx_numeric)
        } else {
            vec![]
        };
        cols_idx_integer.retain(|idx| !cols_idx_decimal_specified.contains(idx));
        cols_idx_integer.extend(cols_idx_integer_specified);
        cols_idx_integer.sort_unstable();
        cols_idx_integer.dedup();
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
            let inferred_num_formats_all = inferred_num_formats(&options.value_plans);
            let inferred_num_formats_slice =
                if inferred_num_formats_all.len() >= sheet_slice.col_end_exclusive {
                    inferred_num_formats_all
                        [sheet_slice.col_start_inclusive..sheet_slice.col_end_exclusive]
                        .to_vec()
                } else {
                    vec![None; sheet_slice.col_end_exclusive - sheet_slice.col_start_inclusive]
                };
            let column_formats_slice = slice_column_format_overrides(
                &options.column_formats,
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
                cols_fmt_overrides: &column_formats_slice,
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
                header_row_count,
            )?;

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
                &fmt_headers,
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
                            &scientific_formats_by_col[col_idx]
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
