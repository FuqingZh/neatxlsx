//! Stateless helper utilities used by the XLSX writer kernel.

use std::collections::{BTreeMap, BTreeSet};

use crate::constant::{
    ColumnIdentifier, LEN_SHEET_NAME_MAX, NCOLS_SHEET_MAX, NROWS_SHEET_MAX, SHEET_NAME_ILLEGAL_CHRS,
};
use crate::spec::{
    CellBorder, CellValue, IntegerCoerceMode, SheetHorizontalMerge, SheetSlice, XlsxReport,
    XlsxRowChunkPolicy, XlsxValuePolicy,
};

////////////////////////////////////////////////////////////////////////////////
// #region CellValueConversion

/// Convert `NaN`/`Inf` to policy string; return error for finite values.
fn convert_nan_inf_to_str(x: f64, value_policy: &XlsxValuePolicy) -> Result<String, String> {
    if x.is_nan() {
        return Ok(value_policy.nan_str.clone());
    }
    if x.is_infinite() {
        return Ok(if x.is_sign_positive() {
            value_policy.posinf_str.clone()
        } else {
            value_policy.neginf_str.clone()
        });
    }
    Err("Input is neither NaN nor Inf.".to_string())
}

fn convert_infinite_number(
    value: f64,
    should_keep_missing_values: bool,
    value_policy: &XlsxValuePolicy,
) -> CellValue {
    if should_keep_missing_values {
        CellValue::String(
            convert_nan_inf_to_str(value, value_policy)
                .unwrap_or_else(|_| value_policy.nan_str.clone()),
        )
    } else {
        CellValue::None
    }
}

fn convert_numeric_cell_to_integer(
    value: f64,
    should_keep_missing_values: bool,
    value_policy: &XlsxValuePolicy,
) -> CellValue {
    if value.is_infinite() {
        return convert_infinite_number(value, should_keep_missing_values, value_policy);
    }

    if value_policy.integer_coerce == IntegerCoerceMode::Coerce {
        return CellValue::Number(value as i64 as f64);
    }

    if value.fract() == 0.0 {
        return CellValue::Number(value);
    }

    CellValue::String(value.to_string())
}

fn convert_string_cell_to_integer(
    value: &str,
    should_keep_missing_values: bool,
    value_policy: &XlsxValuePolicy,
) -> CellValue {
    if let Ok(_val) = value.parse::<i64>() {
        return CellValue::Number(_val as f64);
    }

    if value_policy.integer_coerce == IntegerCoerceMode::Strict {
        return CellValue::String(value.to_owned());
    }

    if let Ok(_val) = value.parse::<f64>() {
        if _val.is_infinite() {
            return convert_infinite_number(_val, should_keep_missing_values, value_policy);
        }
        return CellValue::Number(_val as i64 as f64);
    }

    CellValue::String(value.to_owned())
}

/// Normalize cell value according to numeric/integer flags and value policy.
pub fn convert_cell_value(
    value: &CellValue,
    is_numeric_col: bool,
    is_integer_col: bool,
    should_keep_missing_values: bool,
    value_policy: &XlsxValuePolicy,
) -> CellValue {
    if matches!(value, CellValue::None) {
        return if should_keep_missing_values {
            CellValue::String(value_policy.missing_value_str.clone())
        } else {
            CellValue::None
        };
    }

    if !is_numeric_col {
        return match value {
            CellValue::String(s) => CellValue::String(s.clone()),
            CellValue::Number(n) => CellValue::String(n.to_string()),
            CellValue::None => CellValue::None,
        };
    }

    if is_integer_col {
        return match value {
            CellValue::Number(_val) => {
                convert_numeric_cell_to_integer(*_val, should_keep_missing_values, value_policy)
            }
            CellValue::String(_val) => {
                convert_string_cell_to_integer(_val, should_keep_missing_values, value_policy)
            }
            CellValue::None => CellValue::None,
        };
    }

    match value {
        CellValue::Number(_val) => {
            if _val.is_finite() {
                CellValue::Number(*_val)
            } else {
                convert_infinite_number(*_val, should_keep_missing_values, value_policy)
            }
        }
        CellValue::String(_val) => {
            if let Ok(v) = _val.parse::<f64>() {
                if v.is_finite() {
                    CellValue::Number(v)
                } else {
                    convert_infinite_number(v, should_keep_missing_values, value_policy)
                }
            } else {
                CellValue::String(_val.clone())
            }
        }
        CellValue::None => CellValue::None,
    }
}

// #endregion
////////////////////////////////////////////////////////////////////////////////
// #region DataFrameLikeUtils

/// Validate that `columns` has no duplicated names.
pub fn validate_unique_columns(columns: &[&str]) -> Result<(), String> {
    if columns.len() == columns.iter().collect::<BTreeSet<_>>().len() {
        return Ok(());
    }

    let mut positions_by_name: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (_idx, _name) in columns.iter().enumerate() {
        positions_by_name.entry(_name).or_default().push(_idx);
    }

    let message = positions_by_name
        .iter()
        .filter_map(|(_name, _positions)| {
            if _positions.len() > 1 {
                Some(format!(
                    "{_name:?} x{} at indices {_positions:?}",
                    _positions.len()
                ))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("; ");

    Err(format!("Duplicate column names detected: {message}"))
}

/// Resolve mixed refs (`name` or `index`) to sorted unique indices.
pub fn select_sorted_indices_from_refs(
    columns: &[&str],
    refs: Option<&[ColumnIdentifier]>,
) -> Result<Vec<usize>, String> {
    let Some(refs) = refs else {
        return Ok(vec![]);
    };

    let mut indices = BTreeSet::new();
    for _ref_col in refs {
        match _ref_col {
            ColumnIdentifier::Index(idx) => {
                indices.insert(*idx);
            }
            ColumnIdentifier::Name(name) => {
                let Some(idx) = columns.iter().position(|_colname| _colname == name) else {
                    return Err(format!("Column not found: {name:?}"));
                };
                indices.insert(idx);
            }
        }
    }

    Ok(indices.into_iter().collect())
}

// #endregion
////////////////////////////////////////////////////////////////////////////////
// #region RowChunking

/// Derive row chunk size from dataframe width and chunk policy.
pub fn calculate_row_chunk_size(width_df: usize, policy: &XlsxRowChunkPolicy) -> usize {
    if let Some(fixed_size) = policy.fixed_size {
        return fixed_size;
    }
    if width_df >= policy.width_large {
        return policy.size_large;
    }
    if width_df >= policy.width_medium {
        return policy.size_medium;
    }
    policy.size_default
}

/// Generate `(row_start, row_len)` chunks for `n_rows_total`.
pub fn generate_row_chunks(n_rows_total: usize, size_rows_chunk: usize) -> Vec<(usize, usize)> {
    let mut chunks = Vec::new();
    let mut row_cursor = 0;
    while row_cursor < n_rows_total {
        let rows_per_chunk = usize::min(size_rows_chunk, n_rows_total - row_cursor);
        chunks.push((row_cursor, rows_per_chunk));
        row_cursor += rows_per_chunk;
    }
    chunks
}

// #endregion
////////////////////////////////////////////////////////////////////////////////
// #region SheetNormalization

/// Replace invalid chars and trim to valid Excel sheet name.
pub fn sanitize_sheet_name(name: &str, replace_to: &str) -> String {
    let mut sheet_name = name.to_string();
    for _illegal in SHEET_NAME_ILLEGAL_CHRS {
        sheet_name = sheet_name.replace(_illegal, replace_to);
    }
    sheet_name = sheet_name.trim().to_string();
    if sheet_name.is_empty() {
        sheet_name = "Sheet".to_string();
    }

    sheet_name.chars().take(LEN_SHEET_NAME_MAX).collect()
}

/// Split logical dataframe range into Excel-compliant sheet slices.
pub fn plan_sheet_slices(
    height_df: usize,
    width_df: usize,
    height_header: usize,
    sheet_name: &str,
    report: &mut XlsxReport,
) -> Result<Vec<SheetSlice>, String> {
    if height_header == 0 {
        return Err("height_header must be >= 1.".to_string());
    }

    let max_data_rows = NROWS_SHEET_MAX.checked_sub(height_header).ok_or_else(|| {
        format!("Header too tall: height_header={height_header} exceeds Excel limit.")
    })?;

    if max_data_rows == 0 {
        return Err(format!(
            "Header too tall: height_header={height_header} exceeds Excel limit."
        ));
    }

    let mut col_slices = Vec::new();
    let mut col_start = 0;
    while col_start < width_df {
        let col_end = usize::min(width_df, col_start + NCOLS_SHEET_MAX);
        col_slices.push((col_start, col_end));
        col_start = col_end;
    }

    let mut row_slices = Vec::new();
    let mut row_start = 0;
    while row_start < height_df {
        let row_end = usize::min(height_df, row_start + max_data_rows);
        row_slices.push((row_start, row_end));
        row_start = row_end;
    }

    if row_slices.is_empty() {
        row_slices.push((0, 0));
    }

    let parts_total = col_slices.len() * row_slices.len();

    let mut sheet_slices = Vec::new();
    let mut part_idx = 1;
    for _col_slice in &col_slices {
        let (col_start, col_end) = _col_slice;
        for _row_slice in &row_slices {
            let (row_start, row_end) = _row_slice;
            let part_sheet_name = if parts_total == 1 {
                sheet_name.to_string()
            } else {
                create_sheet_identifier(sheet_name, part_idx)
            };

            sheet_slices.push(SheetSlice {
                sheet_name: part_sheet_name,
                row_start_inclusive: *row_start,
                row_end_exclusive: *row_end,
                col_start_inclusive: *col_start,
                col_end_exclusive: *col_end,
            });
            part_idx += 1;
        }
    }

    if parts_total > 1 {
        report.warn(format!(
            "Excel limit overflow: split into {} sheets (columns-first, then rows).",
            sheet_slices.len()
        ));
    }

    Ok(sheet_slices)
}

/// Create suffixed sheet name (`base_1`, `base_2`, ...), respecting length cap.
fn create_sheet_identifier(base_name: &str, part_idx_1based: usize) -> String {
    let sheet_name_suffix = format!("_{part_idx_1based}");
    let base_name_max_len = LEN_SHEET_NAME_MAX.saturating_sub(sheet_name_suffix.len());

    let sheet_name_base: String = base_name
        .chars()
        .take(usize::max(1, base_name_max_len))
        .collect();

    format!("{sheet_name_base}{sheet_name_suffix}")
}

// #endregion
////////////////////////////////////////////////////////////////////////////////
// #region HeaderMergeUtils

/// Convert sorted indices to contiguous inclusive ranges.
pub fn derive_contiguous_ranges(sorted_indices: &[usize]) -> Vec<(usize, usize)> {
    if sorted_indices.is_empty() {
        return vec![];
    }

    let mut contiguous_ranges = Vec::new();
    let mut idx_start = sorted_indices[0];
    let mut idx_end = sorted_indices[0];

    for _idx in &sorted_indices[1..] {
        let idx = *_idx;
        if idx == idx_end + 1 {
            idx_end = idx;
        } else {
            contiguous_ranges.push((idx_start, idx_end));
            idx_start = idx;
            idx_end = idx;
        }
    }

    contiguous_ranges.push((idx_start, idx_end));
    contiguous_ranges
}

/// Plan horizontal merges for repeated non-empty header text per row.
pub fn plan_horizontal_merges(
    header_grid: &[Vec<String>],
) -> BTreeMap<usize, Vec<SheetHorizontalMerge>> {
    let mut horizontal_merges_by_row = BTreeMap::new();
    if header_grid.is_empty() {
        return horizontal_merges_by_row;
    }

    let row_count = header_grid.len();
    let col_count = header_grid[0].len();

    for _row_idx in header_grid.iter().enumerate().take(row_count) {
        let (row_idx, _) = _row_idx;
        let current_row = &header_grid[row_idx];
        let mut col_idx = 0;

        while col_idx < col_count {
            let cell_value = &current_row[col_idx];
            if cell_value.is_empty() {
                col_idx += 1;
                continue;
            }

            let mut col_idx_end = col_idx + 1;
            while col_idx_end < col_count && current_row[col_idx_end] == *cell_value {
                col_idx_end += 1;
            }

            if col_idx_end - col_idx > 1 {
                horizontal_merges_by_row
                    .entry(row_idx)
                    .or_insert_with(Vec::new)
                    .push(SheetHorizontalMerge {
                        row_idx_start: row_idx,
                        col_idx_start: col_idx,
                        col_idx_end: col_idx_end - 1,
                        text: cell_value.clone(),
                    });
            }
            col_idx = col_idx_end;
        }
    }

    horizontal_merges_by_row
}

/// Generate contiguous vertical runs `(col, row_start, row_end, text)`.
fn _generate_vertical_runs(header_grid: &[Vec<String>]) -> Vec<(usize, usize, usize, String)> {
    let mut run_collection = Vec::new();
    if header_grid.is_empty() {
        return run_collection;
    }
    let Some(header_row_0) = header_grid.first() else {
        return run_collection;
    };

    let row_count = header_grid.len();
    let col_count = header_row_0.len();

    debug_assert!(
        header_grid.iter().all(|_row| _row.len() == col_count),
        "All rows must have the same number of columns."
    );

    for _col_idx in header_row_0.iter().enumerate() {
        let (col_idx, _) = _col_idx;
        let mut row_idx_start = 0;
        while row_idx_start < row_count {
            let cell_value = &header_grid[row_idx_start][col_idx];
            if cell_value.is_empty() {
                row_idx_start += 1;
                continue;
            }

            let mut row_idx_next = row_idx_start + 1;
            while row_idx_next < row_count && header_grid[row_idx_next][col_idx] == *cell_value {
                row_idx_next += 1;
            }

            let vertical_run_len = row_idx_next - row_idx_start;
            if vertical_run_len > 1 {
                run_collection.push((col_idx, row_idx_start, row_idx_next - 1, cell_value.clone()));
            }

            row_idx_start = row_idx_next;
        }
    }

    run_collection
}

/// Build border plan to simulate vertical merge visuals without merge cells.
pub fn plan_vertical_visual_merge_borders(
    header_grid: &[Vec<String>],
) -> BTreeMap<(usize, usize), CellBorder> {
    let mut vertical_merge_border_plan = BTreeMap::new();

    for _run in _generate_vertical_runs(header_grid) {
        let (col_idx, row_start, row_end, _) = _run;
        for _row_idx in row_start..=row_end {
            let row_idx = _row_idx;
            vertical_merge_border_plan.insert(
                (row_idx, col_idx),
                CellBorder {
                    top: if row_idx == row_start { 1 } else { 0 },
                    bottom: if row_idx == row_end { 1 } else { 0 },
                    left: 1,
                    right: 1,
                },
            );
        }
    }

    vertical_merge_border_plan
}

/// Clear repeated text in vertical runs, keeping only first row text.
pub fn apply_vertical_run_text_blankout(header_grid: &mut [Vec<String>]) {
    for (col_idx, row_start, row_end, _) in _generate_vertical_runs(header_grid) {
        for _row in header_grid.iter_mut().take(row_end + 1).skip(row_start + 1) {
            _row[col_idx].clear();
        }
    }
}

/// Build lookup map for cells covered by a horizontal merge (excluding anchor).
pub fn create_horizontal_merge_tracker(
    row_horizontal_merge_mapping: &BTreeMap<usize, Vec<SheetHorizontalMerge>>,
) -> BTreeMap<(usize, usize), bool> {
    let mut merged_cells_tracker = BTreeMap::new();

    for _row_merges in row_horizontal_merge_mapping {
        let (row_idx, horizontal_merges) = _row_merges;
        for _merge in horizontal_merges {
            let merge = _merge;
            for _col_idx in (merge.col_idx_start + 1)..=merge.col_idx_end {
                let col_idx = _col_idx;
                merged_cells_tracker.insert((*row_idx, col_idx), true);
            }
        }
    }

    merged_cells_tracker
}

// #endregion
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constant::ColumnIdentifier;

    #[test]
    fn test_select_sorted_indices_from_refs_respects_typed_selectors() {
        let columns: Vec<&str> = vec!["x", "0", "y"];

        assert_eq!(
            select_sorted_indices_from_refs(
                &columns,
                Some(&[
                    ColumnIdentifier::Name("0".to_string()),
                    ColumnIdentifier::Index(0),
                ]),
            )
            .unwrap(),
            vec![0, 1]
        );
    }

    #[test]
    fn test_select_sorted_indices_from_refs_rejects_missing_name() {
        let columns = vec!["x", "y"];

        let err = select_sorted_indices_from_refs(
            &columns,
            Some(&[ColumnIdentifier::Name("0".to_string())]),
        )
        .unwrap_err();

        assert!(err.contains("Column not found"));
    }

    #[test]
    fn test_generate_vertical_runs_detects_only_contiguous_non_empty_runs() {
        let grid = vec![
            vec!["A".to_string(), "X".to_string()],
            vec!["A".to_string(), "".to_string()],
            vec!["A".to_string(), "X".to_string()],
            vec!["".to_string(), "X".to_string()],
            vec!["B".to_string(), "X".to_string()],
            vec!["B".to_string(), "Y".to_string()],
        ];

        assert_eq!(
            _generate_vertical_runs(&grid),
            vec![
                (0, 0, 2, "A".to_string()),
                (0, 4, 5, "B".to_string()),
                (1, 2, 4, "X".to_string())
            ]
        );
    }

    #[test]
    fn test_apply_vertical_run_text_blankout() {
        let mut grid = vec![
            vec!["A".to_string(), "B".to_string()],
            vec!["A".to_string(), "B".to_string()],
            vec!["".to_string(), "B".to_string()],
            vec!["C".to_string(), "B".to_string()],
            vec!["C".to_string(), "".to_string()],
        ];

        apply_vertical_run_text_blankout(&mut grid);

        assert_eq!(grid[0][0], "A");
        assert_eq!(grid[1][0], "");
        assert_eq!(grid[3][0], "C");
        assert_eq!(grid[4][0], "");

        assert_eq!(grid[0][1], "B");
        assert_eq!(grid[1][1], "");
        assert_eq!(grid[2][1], "");
        assert_eq!(grid[3][1], "");
    }
}
