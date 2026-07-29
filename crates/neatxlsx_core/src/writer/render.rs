//! Worksheet formatting and low-level XLSX rendering.

use std::collections::{BTreeMap, BTreeSet};

use rust_xlsxwriter::{Format, FormatAlign, FormatBorder, Worksheet, XlsxError};

use crate::spec::{
    AutofitMode, AutofitPolicy, CellFormatPatch, CellValue, ColumnFormatPlan, XlsxWriteOptions,
};
use crate::util::{
    apply_vertical_run_text_blankout, create_horizontal_merge_tracker, plan_horizontal_merges,
};

pub(super) struct ColumnFormatPlanOptions<'a> {
    /// Number of columns in current sheet slice.
    pub(super) width_data: usize,
    /// Slice-local numeric column indices.
    pub(super) cols_idx_numeric: &'a [usize],
    /// Slice-local integer column indices.
    pub(super) cols_idx_integer: &'a [usize],
    /// Slice-local explicit decimal column indices.
    pub(super) cols_idx_decimal: Option<&'a [usize]>,
    /// Optional per-column format overrides.
    pub(super) cols_fmt_overrides: &'a BTreeMap<usize, CellFormatPatch>,
    /// Base text format.
    pub(super) fmt_text: &'a CellFormatPatch,
    /// Base integer format.
    pub(super) fmt_integer: &'a CellFormatPatch,
    /// Base decimal format.
    pub(super) fmt_decimal: &'a CellFormatPatch,
    /// Global write options.
    pub(super) options_write: &'a XlsxWriteOptions,
}

pub(super) fn apply_column_widths(
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

/// Build per-column base/final format plans for current sheet slice.
pub(super) fn plan_column_formats(options: ColumnFormatPlanOptions<'_>) -> ColumnFormatPlan {
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

    for col_idx in 0..width_data {
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

pub(super) fn write_header(
    worksheet: &mut Worksheet,
    mut header_grid: Vec<Vec<String>>,
    should_merge: bool,
    fmt_header: &Format,
) -> Result<(), String> {
    if !should_merge {
        for (row_idx, row_values) in header_grid.iter().enumerate() {
            for (col_idx, cell_value) in row_values.iter().enumerate() {
                write_header_cell(worksheet, row_idx, col_idx, cell_value, fmt_header)?;
            }
        }
        return Ok(());
    }

    apply_vertical_run_text_blankout(&mut header_grid);
    let horizontal_merges_by_row = plan_horizontal_merges(&header_grid);
    let horizontal_merge_tracker = create_horizontal_merge_tracker(&horizontal_merges_by_row);

    for (row_idx, row_values) in header_grid.iter().enumerate() {
        for (col_idx, cell_value) in row_values.iter().enumerate() {
            if horizontal_merge_tracker
                .get(&(row_idx, col_idx))
                .copied()
                .unwrap_or(false)
            {
                continue;
            }

            write_header_cell(worksheet, row_idx, col_idx, cell_value, fmt_header)?;
        }

        if let Some(merges) = horizontal_merges_by_row.get(&row_idx) {
            for merge in merges {
                worksheet
                    .merge_range(
                        cast_row_num(row_idx)?,
                        cast_col_num(merge.col_idx_start)?,
                        cast_row_num(row_idx)?,
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

pub(super) fn write_cell_with_format(
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

pub(super) fn create_rust_xlsx_format(spec: &CellFormatPatch) -> Format {
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

pub(super) fn cast_row_num(value: usize) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| format!("row index overflow: {value}"))
}

pub(super) fn cast_col_num(value: usize) -> Result<u16, String> {
    u16::try_from(value).map_err(|_| format!("column index overflow: {value}"))
}

pub(super) fn format_xlsx_error_text(err: XlsxError) -> String {
    format!("xlsx write error: {err}")
}
