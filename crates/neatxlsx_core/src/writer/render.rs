//! Worksheet formatting and low-level XLSX rendering.

use std::collections::{BTreeMap, BTreeSet};

use rust_xlsxwriter::{Format, FormatAlign, FormatBorder, Worksheet, XlsxError};

use crate::spec::{
    AutofitMode, AutofitPolicy, CellFormatPatch, CellValue, ColumnFormatPlan, ColumnValueKind,
    ColumnValuePlan,
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
    /// Explicit Workbook text-role override.
    pub(super) fmt_text_override: &'a CellFormatPatch,
    /// Explicit Workbook integer-role override.
    pub(super) fmt_integer_override: &'a CellFormatPatch,
    /// Explicit Workbook decimal-role override.
    pub(super) fmt_decimal_override: &'a CellFormatPatch,
    /// Inferred number formats by source-column index.
    pub(super) inferred_num_formats: Option<&'a [Option<String>]>,
}

pub(super) fn inferred_num_formats(plans: &[ColumnValuePlan]) -> Vec<Option<String>> {
    plans
        .iter()
        .map(|plan| match plan.kind {
            ColumnValueKind::Decimal => plan.decimal_scale.map(|scale| {
                if scale == 0 {
                    "0".to_string()
                } else {
                    format!("0.{}", "0".repeat(scale))
                }
            }),
            ColumnValueKind::Date => Some("yyyy-mm-dd".to_string()),
            ColumnValueKind::Datetime => Some("yyyy-mm-dd hh:mm:ss.000".to_string()),
            ColumnValueKind::Time => Some("hh:mm:ss.000".to_string()),
            ColumnValueKind::Duration => Some("[h]:mm:ss.000".to_string()),
            _ => None,
        })
        .collect()
}

pub(super) fn apply_column_widths(
    worksheet: &mut Worksheet,
    policy_autofit: &AutofitPolicy,
    header_widths_by_col: &[u16],
    body_widths_by_col: &[u16],
) -> Result<(), String> {
    if matches!(policy_autofit.mode, AutofitMode::None) || header_widths_by_col.is_empty() {
        return Ok(());
    }

    for col_idx in 0..header_widths_by_col.len() {
        let width_recorded = match policy_autofit.mode {
            AutofitMode::Header => header_widths_by_col[col_idx],
            AutofitMode::Body => body_widths_by_col[col_idx],
            AutofitMode::All => {
                u16::max(header_widths_by_col[col_idx], body_widths_by_col[col_idx])
            }
            AutofitMode::None => header_widths_by_col[col_idx],
        };
        let width_final = apply_autofit_pixel_policy(policy_autofit, width_recorded);
        worksheet
            .set_column_autofit_width(cast_col_num(col_idx)?, width_final)
            .map_err(format_xlsx_error_text)?;
    }
    Ok(())
}

fn apply_autofit_pixel_policy(policy_autofit: &AutofitPolicy, recorded: u16) -> u16 {
    let width_min_chars = policy_autofit.width_cell_min.clamp(1, 255);
    let width_max_chars = policy_autofit.width_cell_max.clamp(width_min_chars, 255);
    let width_min = excel_width_to_pixels(width_min_chars);
    let width_max = excel_width_to_pixels(width_max_chars);
    // `cell_autofit_width()` already includes Excel's seven-pixel cell padding.
    // The public padding remains in Excel character units, so it contributes
    // only the additional max-digit-width pixels here.
    let width_padding = u32::try_from(policy_autofit.width_cell_padding)
        .unwrap_or(u32::MAX)
        .saturating_mul(7)
        .min(u32::from(u16::MAX)) as u16;
    recorded
        .saturating_add(width_padding)
        .clamp(width_min, width_max)
}

/// Convert an Excel character-unit column width into the matching Calibri 11
/// pixel width accepted by `Worksheet::set_column_autofit_width()`.
pub(super) fn excel_width_to_pixels(width: usize) -> u16 {
    u32::try_from(width)
        .unwrap_or(u32::MAX)
        .saturating_mul(7)
        .saturating_add(5)
        .min(u32::from(u16::MAX)) as u16
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
        fmt_text_override,
        fmt_integer_override,
        fmt_decimal_override,
        inferred_num_formats,
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

        if let Some(formats) = inferred_num_formats
            && let Some(Some(num_format)) = formats.get(col_idx)
        {
            fmt_base = fmt_base.merge(&CellFormatPatch {
                num_format: Some(num_format.clone()),
                ..Default::default()
            });
        }

        let role_override = if integer_cols_idx.contains(&col_idx) {
            fmt_integer_override
        } else if decimal_cols_idx
            .as_ref()
            .map_or(numeric_cols_idx.contains(&col_idx), |indices| {
                indices.contains(&col_idx)
            })
        {
            fmt_decimal_override
        } else {
            fmt_text_override
        };
        fmt_base = fmt_base.merge(role_override);

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

/// Rebase source-column format patches onto one physical column slice.
pub(super) fn slice_column_format_overrides(
    overrides: &BTreeMap<usize, CellFormatPatch>,
    start: usize,
    end: usize,
) -> BTreeMap<usize, CellFormatPatch> {
    overrides
        .range(start..end)
        .map(|(index, format)| (index - start, format.clone()))
        .collect()
}

/// Build scientific variants while keeping the column rule as the final patch.
pub(super) fn plan_scientific_formats(
    width: usize,
    scientific: &CellFormatPatch,
    column_overrides: &BTreeMap<usize, CellFormatPatch>,
) -> Vec<CellFormatPatch> {
    (0..width)
        .map(|index| {
            column_overrides
                .get(&index)
                .map_or_else(|| scientific.clone(), |patch| scientific.merge(patch))
        })
        .collect()
}

struct HeaderRowFormatPlan {
    base: Format,
    column_overrides: BTreeMap<usize, Format>,
}

pub(super) struct HeaderFormatPlan {
    rows: Vec<HeaderRowFormatPlan>,
    column_count: usize,
}

impl HeaderFormatPlan {
    fn format_for(&self, row_index: usize, column_index: usize) -> &Format {
        let row = &self.rows[row_index];
        row.column_overrides.get(&column_index).unwrap_or(&row.base)
    }
}

/// Materialize one base format per header row and only the overridden cells.
pub(super) fn plan_header_formats(
    base: &CellFormatPatch,
    row_overrides: &[Option<CellFormatPatch>],
    column_overrides: &BTreeMap<usize, CellFormatPatch>,
    row_count: usize,
    column_count: usize,
) -> Result<HeaderFormatPlan, String> {
    if !row_overrides.is_empty() && row_overrides.len() != row_count {
        return Err("header_row_formats length must equal header height.".to_string());
    }
    let rows = (0..row_count)
        .map(|row_index| {
            let row_patch = row_overrides.get(row_index).and_then(Option::as_ref);
            let row_format = row_patch.map_or_else(|| base.clone(), |value| base.merge(value));
            let column_overrides = column_overrides
                .iter()
                .filter(|(column_index, _)| **column_index < column_count)
                .map(|(column_index, value)| {
                    (
                        *column_index,
                        create_rust_xlsx_format(&row_format.merge(value)),
                    )
                })
                .collect();
            HeaderRowFormatPlan {
                base: create_rust_xlsx_format(&row_format),
                column_overrides,
            }
        })
        .collect();
    Ok(HeaderFormatPlan { rows, column_count })
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
    fmt_headers: &HeaderFormatPlan,
) -> Result<(), String> {
    if fmt_headers.rows.len() != header_grid.len() {
        return Err("header format count must equal header row count.".to_string());
    }
    if header_grid
        .iter()
        .any(|row| row.len() != fmt_headers.column_count)
    {
        return Err("header format width must equal header width.".to_string());
    }
    if !should_merge {
        for (row_idx, row_values) in header_grid.iter().enumerate() {
            for (col_idx, cell_value) in row_values.iter().enumerate() {
                write_header_cell(
                    worksheet,
                    row_idx,
                    col_idx,
                    cell_value,
                    fmt_headers.format_for(row_idx, col_idx),
                )?;
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

            write_header_cell(
                worksheet,
                row_idx,
                col_idx,
                cell_value,
                fmt_headers.format_for(row_idx, col_idx),
            )?;
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
                        fmt_headers.format_for(row_idx, merge.col_idx_start),
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
        CellValue::Blank => {
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
        CellValue::Boolean(val) => {
            worksheet
                .write_boolean_with_format(
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

#[cfg(test)]
mod tests {
    use super::{
        ColumnFormatPlanOptions, apply_autofit_pixel_policy, excel_width_to_pixels,
        plan_column_formats, plan_header_formats, plan_scientific_formats,
        slice_column_format_overrides,
    };
    use crate::spec::CellFormatPatch;
    use std::collections::BTreeMap;

    #[test]
    fn excel_width_pixel_conversion_matches_autofit_column_units() {
        assert_eq!(excel_width_to_pixels(1), 12);
        assert_eq!(excel_width_to_pixels(8), 61);
        assert_eq!(excel_width_to_pixels(60), 425);
        assert_eq!(excel_width_to_pixels(usize::MAX), u16::MAX);
    }

    #[test]
    fn autofit_pixel_policy_clamps_extreme_configuration_without_overflow() {
        let policy = crate::spec::AutofitPolicy {
            width_cell_min: usize::MAX,
            width_cell_max: 0,
            width_cell_padding: usize::MAX,
            ..Default::default()
        };
        assert_eq!(
            apply_autofit_pixel_policy(&policy, 0),
            excel_width_to_pixels(255)
        );
    }

    #[test]
    fn column_rule_wins_after_inferred_and_workbook_role_formats() {
        let overrides = BTreeMap::from([(
            0,
            CellFormatPatch {
                font_name: Some("SimSun".to_string()),
                ..Default::default()
            },
        )]);
        let inferred = vec![Some("0.00".to_string())];
        let plan = plan_column_formats(ColumnFormatPlanOptions {
            width_data: 1,
            cols_idx_numeric: &[0],
            cols_idx_integer: &[],
            cols_idx_decimal: Some(&[0]),
            cols_fmt_overrides: &overrides,
            fmt_text: &CellFormatPatch::default(),
            fmt_integer: &CellFormatPatch::default(),
            fmt_decimal: &CellFormatPatch {
                font_name: Some("Times New Roman".to_string()),
                ..Default::default()
            },
            fmt_text_override: &CellFormatPatch::default(),
            fmt_integer_override: &CellFormatPatch::default(),
            fmt_decimal_override: &CellFormatPatch {
                italic: Some(true),
                ..Default::default()
            },
            inferred_num_formats: Some(&inferred),
        });

        assert_eq!(
            plan.fmts_base_by_col[0].font_name.as_deref(),
            Some("Times New Roman")
        );
        assert_eq!(plan.fmts_base_by_col[0].num_format.as_deref(), Some("0.00"));
        assert_eq!(plan.fmts_base_by_col[0].italic, Some(true));
        assert_eq!(plan.fmts_by_col[0].font_name.as_deref(), Some("SimSun"));
        assert_eq!(plan.fmts_by_col[0].num_format.as_deref(), Some("0.00"));
        assert_eq!(plan.fmts_by_col[0].italic, Some(true));

        let scientific = plan_scientific_formats(
            1,
            &CellFormatPatch {
                font_name: Some("Arial".to_string()),
                num_format: Some("0.0E+0".to_string()),
                ..Default::default()
            },
            &overrides,
        );
        assert_eq!(scientific[0].font_name.as_deref(), Some("SimSun"));
        assert_eq!(scientific[0].num_format.as_deref(), Some("0.0E+0"));
        assert_eq!(scientific[0].italic, None);

        let without_override = plan_scientific_formats(
            1,
            &CellFormatPatch {
                font_name: Some("Arial".to_string()),
                num_format: Some("0.0E+0".to_string()),
                ..Default::default()
            },
            &BTreeMap::new(),
        );
        assert_eq!(without_override[0].font_name.as_deref(), Some("Arial"));
        assert_eq!(without_override[0].italic, None);
    }

    #[test]
    fn column_rules_are_rebased_for_every_physical_column_part() {
        let overrides = BTreeMap::from([
            (
                0,
                CellFormatPatch {
                    font_name: Some("Times New Roman".to_string()),
                    ..Default::default()
                },
            ),
            (
                16_384,
                CellFormatPatch {
                    font_name: Some("SimSun".to_string()),
                    ..Default::default()
                },
            ),
        ]);

        let first = slice_column_format_overrides(&overrides, 0, 16_384);
        let second = slice_column_format_overrides(&overrides, 16_384, 16_385);

        assert_eq!(
            first.get(&0).and_then(|value| value.font_name.as_deref()),
            Some("Times New Roman")
        );
        assert_eq!(
            second.get(&0).and_then(|value| value.font_name.as_deref()),
            Some("SimSun")
        );
    }

    #[test]
    fn header_column_rules_are_rebased_for_every_physical_column_part() {
        let overrides = BTreeMap::from([
            (
                0,
                CellFormatPatch {
                    font_name: Some("Times New Roman".to_string()),
                    ..Default::default()
                },
            ),
            (
                16_384,
                CellFormatPatch {
                    font_name: Some("SimSun".to_string()),
                    ..Default::default()
                },
            ),
        ]);

        let first = slice_column_format_overrides(&overrides, 0, 16_384);
        let second = slice_column_format_overrides(&overrides, 16_384, 16_385);

        assert_eq!(
            first.get(&0).and_then(|value| value.font_name.as_deref()),
            Some("Times New Roman")
        );
        assert_eq!(
            second.get(&0).and_then(|value| value.font_name.as_deref()),
            Some("SimSun")
        );
    }

    #[test]
    fn wide_header_format_plans_only_materialize_column_overrides() {
        let base = CellFormatPatch::default();
        let row_overrides = vec![None, None];
        let without_column_overrides =
            plan_header_formats(&base, &row_overrides, &BTreeMap::new(), 2, 16_385)
                .expect("wide header format plan");

        assert_eq!(without_column_overrides.rows.len(), 2);
        assert_eq!(without_column_overrides.column_count, 16_385);
        assert!(
            without_column_overrides
                .rows
                .iter()
                .all(|row| row.column_overrides.is_empty())
        );

        let column_overrides = BTreeMap::from([(
            16_384,
            CellFormatPatch {
                font_name: Some("SimSun".to_string()),
                ..Default::default()
            },
        )]);
        let with_one_column_override =
            plan_header_formats(&base, &row_overrides, &column_overrides, 2, 16_385)
                .expect("sparse header format plan");

        assert!(with_one_column_override.rows.iter().all(|row| {
            row.column_overrides.len() == 1 && row.column_overrides.contains_key(&16_384)
        }));
    }
}
