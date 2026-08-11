//! Sheet planning, schema inference, and autofit scanning.

use std::collections::BTreeSet;

use polars::prelude::DataFrame;

use crate::spec::{
    AutofitMode, CellValue, ColumnValueKind, SheetSlice, XlsxReport, XlsxValuePolicy,
    XlsxWriteOptions,
};
use crate::util::{
    calculate_row_chunk_size, convert_cell_value, plan_sheet_slices, sanitize_sheet_name,
    select_sorted_indices_from_refs, validate_unique_columns,
};

use super::value::{
    convert_arrow_value_with_plan, dataframe_from_record_batch, estimate_width_len,
    is_scientific_candidate_col, select_integer_column_indices, select_numeric_column_indices,
};
use super::{XlsxRecordBatch, XlsxSheetWriteOptions};

#[derive(Debug, Clone)]
pub struct XlsxSheetPlan {
    pub(super) col_names: Vec<String>,
    pub(super) height_body: usize,
    pub(super) header_grid: Vec<Vec<String>>,
    pub(super) cols_idx_numeric: Vec<usize>,
    pub(super) cols_idx_integer: Vec<usize>,
    pub(super) cols_idx_decimal_specified: Vec<usize>,
    pub(super) header_widths_by_col: Vec<usize>,
    pub(super) body_widths_by_col: Vec<usize>,
    pub(super) sheet_slices: Vec<SheetSlice>,
    pub(super) num_frozen_rows: usize,
    pub(super) should_keep_missing_values: bool,
}

pub(super) struct XlsxSheetPlanBuilder<'a> {
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

impl<'a> XlsxSheetPlanBuilder<'a> {
    pub(super) fn new(
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

    pub(super) fn scan_batch(&mut self, batch: XlsxRecordBatch) -> Result<(), String> {
        let df_batch = dataframe_from_record_batch(batch.clone())?;
        self.ensure_initialized(&df_batch)?;

        let should_scan_body_width = matches!(
            self.options.policy_autofit.mode,
            AutofitMode::Body | AutofitMode::All
        );
        if should_scan_body_width && df_batch.width() > 0 {
            self.scan_body_widths(&batch)?;
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
        let cols_idx_integer_specified =
            select_sorted_indices_from_refs(&col_names_ref, self.options.cols_integer.as_deref())?;
        self.cols_idx_decimal_specified =
            select_sorted_indices_from_refs(&col_names_ref, self.options.cols_decimal.as_deref())?;
        self.cols_idx_numeric
            .extend(cols_idx_integer_specified.iter().copied());
        self.cols_idx_numeric
            .extend(self.cols_idx_decimal_specified.iter().copied());
        self.cols_idx_numeric.sort_unstable();
        self.cols_idx_numeric.dedup();

        let rows_chunk =
            calculate_row_chunk_size(self.width_body, &self.options_write.row_chunk_policy);
        if rows_chunk == 0 {
            return Err("row_chunk_policy resolved to 0 rows; expected >= 1.".to_string());
        }

        self.cols_idx_integer = if self.options_write.should_infer_integer_cols {
            select_integer_column_indices(df_batch, &self.cols_idx_numeric)
        } else {
            vec![]
        };
        self.cols_idx_integer
            .retain(|idx| !self.cols_idx_decimal_specified.contains(idx));
        self.cols_idx_integer.extend(cols_idx_integer_specified);
        self.cols_idx_integer.sort_unstable();
        self.cols_idx_integer.dedup();

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

    fn scan_body_widths(&mut self, batch: &XlsxRecordBatch) -> Result<(), String> {
        let Some(max_rows) = self.options.policy_autofit.height_body_inferred_max else {
            return self.scan_body_width_rows(batch, batch.len());
        };
        if self.rows_seen_for_autofit >= max_rows {
            return Ok(());
        }
        let remaining = max_rows - self.rows_seen_for_autofit;
        let rows_to_scan = usize::min(remaining, batch.len());
        self.scan_body_width_rows(batch, rows_to_scan)
    }

    fn scan_body_width_rows(
        &mut self,
        batch: &XlsxRecordBatch,
        rows_to_scan: usize,
    ) -> Result<(), String> {
        let numeric_cols_idx: BTreeSet<usize> = self.cols_idx_numeric.iter().copied().collect();
        let integer_cols_idx: BTreeSet<usize> = self.cols_idx_integer.iter().copied().collect();
        let decimal_cols_idx: BTreeSet<usize> =
            self.cols_idx_decimal_specified.iter().copied().collect();
        let is_decimal_explicit = !decimal_cols_idx.is_empty();

        for row_local in 0..rows_to_scan {
            for (col_idx, col) in batch.arrays().iter().enumerate() {
                let is_numeric_col = numeric_cols_idx.contains(&col_idx);
                let is_integer_col = integer_cols_idx.contains(&col_idx);
                let is_decimal_specified = decimal_cols_idx.contains(&col_idx);
                let is_scientific_candidate = is_scientific_candidate_col(
                    &self.options.policy_scientific,
                    is_integer_col,
                    is_decimal_explicit,
                    is_decimal_specified,
                );
                let value_raw = convert_arrow_value_with_plan(
                    col.as_ref(),
                    row_local,
                    self.options.value_plans.get(col_idx),
                )?;
                let value = if value_raw.warning.is_some() {
                    value_raw.value
                } else {
                    match self.options.value_plans.get(col_idx).map(|plan| plan.kind) {
                        Some(
                            ColumnValueKind::Boolean
                            | ColumnValueKind::Date
                            | ColumnValueKind::Datetime
                            | ColumnValueKind::Time
                            | ColumnValueKind::Duration,
                        ) => value_raw.value,
                        _ => convert_cell_value(
                            &value_raw.value,
                            is_numeric_col,
                            is_integer_col,
                            self.should_keep_missing_values,
                            &self.value_policy,
                        ),
                    }
                };
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

    pub(super) fn finish(self) -> Result<XlsxSheetPlan, String> {
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

pub(super) fn calculate_slice_indices(
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
