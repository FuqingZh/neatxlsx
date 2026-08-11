//! DataFrame and Arrow value normalization for XLSX planning and rendering.

use std::io::Cursor;

use arrow::array::{
    Array as ArrowArray, BooleanArray, Int128Array, Int256Array, PrimitiveArray, TryExtend,
    Utf8Array, Utf8ViewArray,
};
use arrow::datatypes::{ArrowDataType, ArrowSchema};
use chrono::{Duration as ChronoDuration, NaiveDate, Timelike};
use polars::prelude::{AnyValue, DataFrame, IpcReader, SerReader};

use crate::spec::{
    AutofitPolicy, CellValue, ColumnValueKind, ColumnValuePlan, ScientificPolicy, ScientificScope,
    WarningCode, XlsxValuePolicy,
};

use super::XlsxRecordBatch;

pub(super) fn dataframe_from_record_batch(batch: XlsxRecordBatch) -> Result<DataFrame, String> {
    let schema_arrow = batch.schema().clone();
    let mut df = DataFrame::empty_with_arrow_schema(&schema_arrow);
    df.try_extend(std::iter::once(batch))
        .map_err(|err| format!("Failed to convert Arrow record batch to DataFrame: {err}"))?;
    Ok(df)
}

/// Estimate displayed width units for one normalized cell value.
///
/// Used by autofit inference logic.
pub(super) fn estimate_width_len(
    value: &CellValue,
    is_numeric_col: bool,
    is_integer_col: bool,
    is_scientific_candidate: bool,
    policy_scientific: &ScientificPolicy,
    should_keep_missing_values: bool,
    value_policy: &XlsxValuePolicy,
) -> usize {
    match value {
        CellValue::Blank => {
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
        CellValue::Boolean(value) => {
            if *value {
                4
            } else {
                5
            }
        }
    }
}

fn estimate_unicode_string_width(s: &str) -> usize {
    let ascii_count = s.chars().filter(|chr| chr.is_ascii()).count();
    let non_ascii_count = s.chars().count().saturating_sub(ascii_count);
    ascii_count + (non_ascii_count as f64 * 1.6).round() as usize
}

pub(super) fn read_dataframe_from_ipc_bytes(ipc_df: &[u8]) -> Result<DataFrame, String> {
    IpcReader::new(Cursor::new(ipc_df))
        .finish()
        .map_err(|err| format!("Failed to read IPC DataFrame bytes: {err}"))
}

pub(super) fn validate_policy_autofit(policy_autofit: &AutofitPolicy) -> Result<(), String> {
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

pub(super) fn validate_policy_scientific(
    policy_scientific: &ScientificPolicy,
) -> Result<(), String> {
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

pub(super) fn is_scientific_candidate_col(
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

pub(super) fn should_use_scientific_value(
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

pub(super) fn select_numeric_column_indices(df: &DataFrame) -> Vec<usize> {
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

pub(super) fn select_integer_column_indices(
    df: &DataFrame,
    cols_idx_numeric: &[usize],
) -> Vec<usize> {
    cols_idx_numeric
        .iter()
        .copied()
        .filter(|idx| df.get_columns()[*idx].dtype().is_integer())
        .collect()
}

pub(super) fn select_numeric_column_indices_from_arrow_schema(schema: &ArrowSchema) -> Vec<usize> {
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

pub(super) fn select_integer_column_indices_from_arrow_schema(
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

pub(super) fn extract_string_grid_from_dataframe(
    df: &DataFrame,
) -> Result<Vec<Vec<String>>, String> {
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

pub(super) fn convert_any_value_to_cell_value(value: AnyValue<'_>) -> CellValue {
    match value {
        AnyValue::Null => CellValue::Blank,
        AnyValue::String(val) => CellValue::String(val.to_string()),
        AnyValue::StringOwned(val) => CellValue::String(val.to_string()),
        AnyValue::Boolean(val) => CellValue::Boolean(val),
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

pub(super) fn convert_arrow_value_to_cell_value(
    array: &dyn ArrowArray,
    row_idx: usize,
) -> Result<CellValue, String> {
    if array.is_null(row_idx) {
        return Ok(CellValue::Blank);
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
        ArrowDataType::Null => Ok(CellValue::Blank),
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
        dtype => Err(format!(
            "Unsupported Arrow dtype for string value: {dtype:?}"
        )),
    }
}

/// A converted value and the warning category, if exact text was required.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ConvertedCell {
    pub(super) value: CellValue,
    pub(super) warning: Option<WarningCode>,
}

fn warning_cell(value: CellValue, warning: WarningCode) -> ConvertedCell {
    ConvertedCell {
        value,
        warning: Some(warning),
    }
}

fn clean_decimal_digits(value: &str) -> String {
    let digits = value.trim_start_matches('-').trim_start_matches('+');
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        "0".to_string()
    } else {
        digits.to_string()
    }
}

fn integer_cell_from_text(value: String) -> CellValue {
    let digits = clean_decimal_digits(&value);
    if (digits == "0" || digits.len() <= 15)
        && let Ok(number) = value.parse::<f64>()
    {
        return CellValue::Number(number);
    }
    CellValue::String(value)
}

fn fixed_point_text(raw: &str, scale: usize) -> String {
    let negative = raw.starts_with('-');
    let digits = raw.trim_start_matches('-').trim_start_matches('+');
    let digits = if digits.is_empty() { "0" } else { digits };
    let body = if scale == 0 {
        digits.to_string()
    } else if digits.len() <= scale {
        format!("0.{}{}", "0".repeat(scale - digits.len()), digits)
    } else {
        let split = digits.len() - scale;
        format!("{}.{}", &digits[..split], &digits[split..])
    };
    if negative && body != "0" {
        format!("-{body}")
    } else {
        body
    }
}

fn decimal_cell_from_text(raw: String, scale: usize) -> ConvertedCell {
    let fixed = fixed_point_text(&raw, scale);
    let significant = clean_decimal_digits(&raw).len();
    if significant <= 15
        && scale <= 15
        && let Ok(number) = fixed.parse::<f64>()
    {
        return ConvertedCell {
            value: CellValue::Number(number),
            warning: None,
        };
    }
    warning_cell(CellValue::String(fixed), WarningCode::PrecisionAsText)
}

fn temporal_unit_nanos(unit: &str) -> Option<i128> {
    match unit {
        "s" | "second" | "seconds" => Some(1_000_000_000),
        "ms" | "millisecond" | "milliseconds" => Some(1_000_000),
        "us" | "microsecond" | "microseconds" => Some(1_000),
        "ns" | "nanosecond" | "nanoseconds" => Some(1),
        _ => None,
    }
}

fn temporal_unit_suffix(unit: &str) -> &'static str {
    match unit {
        "s" | "second" | "seconds" => "s",
        "ms" | "millisecond" | "milliseconds" => "ms",
        "us" | "microsecond" | "microseconds" => "us",
        _ => "ns",
    }
}

fn source_precision_digits(unit: &str) -> usize {
    match unit {
        "s" | "second" | "seconds" => 0,
        "ms" | "millisecond" | "milliseconds" => 3,
        "us" | "microsecond" | "microseconds" => 6,
        _ => 9,
    }
}

fn format_fractional(nanos: u32, digits: usize) -> String {
    if digits == 0 {
        String::new()
    } else {
        format!(".{:09}", nanos)[..digits + 1].to_string()
    }
}

fn naive_datetime_from_raw(raw: i64, unit: &str) -> Option<chrono::NaiveDateTime> {
    let scale = temporal_unit_nanos(unit)?;
    let total_nanos = i128::from(raw).checked_mul(scale)?;
    let seconds = total_nanos.div_euclid(1_000_000_000);
    let nanos = total_nanos.rem_euclid(1_000_000_000) as u32;
    let seconds = i64::try_from(seconds).ok()?;
    chrono::DateTime::from_timestamp(seconds, nanos).map(|value| value.naive_utc())
}

fn excel_serial_for_date(date: NaiveDate) -> Option<f64> {
    let min = NaiveDate::from_ymd_opt(1900, 1, 1)?;
    let max = NaiveDate::from_ymd_opt(9999, 12, 31)?;
    if date < min || date > max {
        return None;
    }
    let epoch = NaiveDate::from_ymd_opt(1899, 12, 31)?;
    let mut days = date.signed_duration_since(epoch).num_days();
    if date >= NaiveDate::from_ymd_opt(1900, 3, 1)? {
        days += 1;
    }
    Some(days as f64)
}

fn safe_excel_day_fraction(total_millis: i128) -> Option<f64> {
    let serial = total_millis as f64 / 86_400_000.0;
    if !serial.is_finite() {
        return None;
    }
    let round_trip = (serial * 86_400_000.0).round() as i128;
    (round_trip == total_millis).then_some(serial)
}

fn date_value(raw: i32) -> ConvertedCell {
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).expect("valid epoch");
    let Some(date) = epoch.checked_add_signed(ChronoDuration::days(i64::from(raw))) else {
        return warning_cell(
            CellValue::String(raw.to_string()),
            WarningCode::TemporalAsText,
        );
    };
    if let Some(serial) = excel_serial_for_date(date) {
        ConvertedCell {
            value: CellValue::Number(serial),
            warning: None,
        }
    } else {
        warning_cell(
            CellValue::String(date.format("%Y-%m-%d").to_string()),
            WarningCode::TemporalAsText,
        )
    }
}

fn datetime_value(raw: i64, unit: &str) -> ConvertedCell {
    let Some(datetime) = naive_datetime_from_raw(raw, unit) else {
        return warning_cell(
            CellValue::String(format!("{raw}{}", temporal_unit_suffix(unit))),
            WarningCode::TemporalAsText,
        );
    };
    let date = datetime.date();
    let Some(date_serial) = excel_serial_for_date(date) else {
        return warning_cell(
            CellValue::String(format_datetime_text(datetime, unit)),
            WarningCode::TemporalAsText,
        );
    };
    let millis = i128::from(datetime.and_utc().timestamp()) * 1_000
        + i128::from(datetime.and_utc().timestamp_subsec_millis());
    let sub_millis = match temporal_unit_nanos(unit) {
        Some(scale) => (i128::from(raw) * scale).rem_euclid(1_000_000) != 0,
        None => true,
    };
    if sub_millis {
        return warning_cell(
            CellValue::String(format_datetime_text(datetime, unit)),
            WarningCode::TemporalAsText,
        );
    }
    let fraction = date_serial + (millis.rem_euclid(86_400_000) as f64 / 86_400_000.0);
    let serial_millis = ((fraction - date_serial) * 86_400_000.0).round() as i128;
    if serial_millis != millis.rem_euclid(86_400_000) {
        return warning_cell(
            CellValue::String(format_datetime_text(datetime, unit)),
            WarningCode::TemporalAsText,
        );
    }
    ConvertedCell {
        value: CellValue::Number(fraction),
        warning: None,
    }
}

fn format_datetime_text(datetime: chrono::NaiveDateTime, unit: &str) -> String {
    format!(
        "{}{}",
        datetime.format("%Y-%m-%d %H:%M:%S"),
        format_fractional(datetime.nanosecond(), source_precision_digits(unit))
    )
}

fn time_value(raw: i64, unit: &str) -> ConvertedCell {
    let Some(scale) = temporal_unit_nanos(unit) else {
        return warning_cell(
            CellValue::String(format!("{raw}{}", temporal_unit_suffix(unit))),
            WarningCode::TemporalAsText,
        );
    };
    let total_nanos = i128::from(raw) * scale;
    if !(0..86_400_000_000_000i128).contains(&total_nanos) || total_nanos.rem_euclid(1_000_000) != 0
    {
        return warning_cell(
            CellValue::String(format_time_text(total_nanos, unit)),
            WarningCode::TemporalAsText,
        );
    }
    let millis = total_nanos / 1_000_000;
    let Some(serial) = safe_excel_day_fraction(millis) else {
        return warning_cell(
            CellValue::String(format_time_text(total_nanos, unit)),
            WarningCode::TemporalAsText,
        );
    };
    ConvertedCell {
        value: CellValue::Number(serial),
        warning: None,
    }
}

fn format_time_text(total_nanos: i128, unit: &str) -> String {
    let nanos = total_nanos.rem_euclid(86_400_000_000_000);
    let hours = nanos / 3_600_000_000_000;
    let minutes = (nanos / 60_000_000_000) % 60;
    let seconds = (nanos / 1_000_000_000) % 60;
    let fraction = format_fractional(
        (nanos % 1_000_000_000) as u32,
        source_precision_digits(unit),
    );
    format!("{hours:02}:{minutes:02}:{seconds:02}{fraction}")
}

fn duration_value(raw: i64, unit: &str) -> ConvertedCell {
    let Some(scale) = temporal_unit_nanos(unit) else {
        return warning_cell(
            CellValue::String(format!("{raw}{}", temporal_unit_suffix(unit))),
            WarningCode::TemporalAsText,
        );
    };
    let total_nanos = i128::from(raw) * scale;
    if total_nanos < 0 || total_nanos.rem_euclid(1_000_000) != 0 {
        return warning_cell(
            CellValue::String(format!("{raw}{}", temporal_unit_suffix(unit))),
            WarningCode::TemporalAsText,
        );
    }
    let Some(serial) = safe_excel_day_fraction(total_nanos / 1_000_000) else {
        return warning_cell(
            CellValue::String(format!("{raw}{}", temporal_unit_suffix(unit))),
            WarningCode::TemporalAsText,
        );
    };
    ConvertedCell {
        value: CellValue::Number(serial),
        warning: None,
    }
}

fn plan_kind_from_arrow_dtype(dtype: &ArrowDataType) -> ColumnValueKind {
    match dtype {
        ArrowDataType::Null => ColumnValueKind::Null,
        ArrowDataType::Boolean => ColumnValueKind::Boolean,
        ArrowDataType::Int8
        | ArrowDataType::Int16
        | ArrowDataType::Int32
        | ArrowDataType::Int64
        | ArrowDataType::Int128
        | ArrowDataType::UInt8
        | ArrowDataType::UInt16
        | ArrowDataType::UInt32
        | ArrowDataType::UInt64 => ColumnValueKind::Integer,
        ArrowDataType::Float16 | ArrowDataType::Float32 | ArrowDataType::Float64 => {
            ColumnValueKind::Float
        }
        ArrowDataType::Decimal(..)
        | ArrowDataType::Decimal32(..)
        | ArrowDataType::Decimal64(..)
        | ArrowDataType::Decimal256(..) => ColumnValueKind::Decimal,
        ArrowDataType::Date32 | ArrowDataType::Date64 => ColumnValueKind::Date,
        ArrowDataType::Timestamp(_, _) => ColumnValueKind::Datetime,
        ArrowDataType::Time32(_) | ArrowDataType::Time64(_) => ColumnValueKind::Time,
        ArrowDataType::Duration(_) => ColumnValueKind::Duration,
        _ => ColumnValueKind::String,
    }
}

fn plan_unit_from_arrow_dtype(dtype: &ArrowDataType) -> Option<String> {
    match dtype {
        ArrowDataType::Timestamp(unit, _) | ArrowDataType::Duration(unit) => {
            Some(format!("{unit:?}").to_ascii_lowercase())
        }
        ArrowDataType::Time32(unit) | ArrowDataType::Time64(unit) => {
            Some(format!("{unit:?}").to_ascii_lowercase())
        }
        _ => None,
    }
}

fn effective_plan(array: &dyn ArrowArray, plan: Option<&ColumnValuePlan>) -> ColumnValuePlan {
    if let Some(plan) = plan {
        return plan.clone();
    }
    let dtype = array.dtype();
    let (decimal_precision, decimal_scale) = match dtype {
        ArrowDataType::Decimal(precision, scale)
        | ArrowDataType::Decimal32(precision, scale)
        | ArrowDataType::Decimal64(precision, scale)
        | ArrowDataType::Decimal256(precision, scale) => (Some(*precision), Some(*scale)),
        _ => (None, None),
    };
    ColumnValuePlan {
        name: String::new(),
        kind: plan_kind_from_arrow_dtype(dtype),
        unit: plan_unit_from_arrow_dtype(dtype),
        timezone: match dtype {
            ArrowDataType::Timestamp(_, Some(tz)) => Some(tz.to_string()),
            _ => None,
        },
        decimal_precision,
        decimal_scale,
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn convert_arrow_value_with_plan(
    array: &dyn ArrowArray,
    row_idx: usize,
    plan: Option<&ColumnValuePlan>,
) -> Result<ConvertedCell, String> {
    if array.is_null(row_idx) {
        return Ok(ConvertedCell {
            value: CellValue::Blank,
            warning: None,
        });
    }
    let plan = effective_plan(array, plan);
    match plan.kind {
        ColumnValueKind::Null => Ok(ConvertedCell {
            value: CellValue::Blank,
            warning: None,
        }),
        ColumnValueKind::Boolean => {
            let arr = array
                .as_any()
                .downcast_ref::<BooleanArray>()
                .ok_or_else(|| {
                    format!(
                        "Failed to downcast Boolean array with dtype {:?}",
                        array.dtype()
                    )
                })?;
            Ok(ConvertedCell {
                value: CellValue::Boolean(arr.value(row_idx)),
                warning: None,
            })
        }
        ColumnValueKind::Integer => {
            macro_rules! integer {
                ($ty:ty) => {{
                    let arr = array
                        .as_any()
                        .downcast_ref::<PrimitiveArray<$ty>>()
                        .ok_or_else(|| {
                            format!(
                                "Failed to downcast integer array with dtype {:?}",
                                array.dtype()
                            )
                        })?;
                    let raw = arr.value(row_idx).to_string();
                    Ok(ConvertedCell {
                        value: integer_cell_from_text(raw),
                        warning: match integer_cell_from_text(arr.value(row_idx).to_string()) {
                            CellValue::String(_) => Some(WarningCode::PrecisionAsText),
                            _ => None,
                        },
                    })
                }};
            }
            match array.dtype() {
                ArrowDataType::Int8 => integer!(i8),
                ArrowDataType::Int16 => integer!(i16),
                ArrowDataType::Int32 => integer!(i32),
                ArrowDataType::Int64 => integer!(i64),
                ArrowDataType::Int128 => integer!(i128),
                ArrowDataType::UInt8 => integer!(u8),
                ArrowDataType::UInt16 => integer!(u16),
                ArrowDataType::UInt32 => integer!(u32),
                ArrowDataType::UInt64 => integer!(u64),
                _ => Err(format!(
                    "Unsupported integer array dtype {:?}",
                    array.dtype()
                )),
            }
        }
        ColumnValueKind::Float => {
            macro_rules! float {
                ($ty:ty) => {{
                    let arr = array
                        .as_any()
                        .downcast_ref::<PrimitiveArray<$ty>>()
                        .ok_or_else(|| {
                            format!(
                                "Failed to downcast float array with dtype {:?}",
                                array.dtype()
                            )
                        })?;
                    Ok(ConvertedCell {
                        value: CellValue::Number(arr.value(row_idx) as f64),
                        warning: None,
                    })
                }};
            }
            match array.dtype() {
                ArrowDataType::Float32 => float!(f32),
                ArrowDataType::Float64 => float!(f64),
                _ => Err(format!("Unsupported float array dtype {:?}", array.dtype())),
            }
        }
        ColumnValueKind::Decimal => {
            let scale = plan.decimal_scale.unwrap_or(0);
            match array.dtype() {
                ArrowDataType::Decimal(_, _) => {
                    let arr = array
                        .as_any()
                        .downcast_ref::<Int128Array>()
                        .ok_or_else(|| {
                            format!(
                                "Failed to downcast Decimal array with dtype {:?}",
                                array.dtype()
                            )
                        })?;
                    Ok(decimal_cell_from_text(
                        arr.value(row_idx).to_string(),
                        scale,
                    ))
                }
                ArrowDataType::Decimal32(_, _) => {
                    let arr = array
                        .as_any()
                        .downcast_ref::<PrimitiveArray<i32>>()
                        .ok_or_else(|| {
                            format!(
                                "Failed to downcast Decimal32 array with dtype {:?}",
                                array.dtype()
                            )
                        })?;
                    Ok(decimal_cell_from_text(
                        arr.value(row_idx).to_string(),
                        scale,
                    ))
                }
                ArrowDataType::Decimal64(_, _) => {
                    let arr = array
                        .as_any()
                        .downcast_ref::<PrimitiveArray<i64>>()
                        .ok_or_else(|| {
                            format!(
                                "Failed to downcast Decimal64 array with dtype {:?}",
                                array.dtype()
                            )
                        })?;
                    Ok(decimal_cell_from_text(
                        arr.value(row_idx).to_string(),
                        scale,
                    ))
                }
                ArrowDataType::Decimal256(_, _) => {
                    let arr = array
                        .as_any()
                        .downcast_ref::<Int256Array>()
                        .ok_or_else(|| {
                            format!(
                                "Failed to downcast Decimal256 array with dtype {:?}",
                                array.dtype()
                            )
                        })?;
                    Ok(decimal_cell_from_text(
                        arr.value(row_idx).to_string(),
                        scale,
                    ))
                }
                _ => Err(format!(
                    "Unsupported decimal array dtype {:?}",
                    array.dtype()
                )),
            }
        }
        ColumnValueKind::String => {
            convert_arrow_value_to_cell_value(array, row_idx).map(|value| ConvertedCell {
                value,
                warning: None,
            })
        }
        ColumnValueKind::Date => match array.dtype() {
            ArrowDataType::Date32 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<PrimitiveArray<i32>>()
                    .ok_or_else(|| {
                        format!(
                            "Failed to downcast Date32 array with dtype {:?}",
                            array.dtype()
                        )
                    })?;
                Ok(date_value(arr.value(row_idx)))
            }
            ArrowDataType::Date64 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<PrimitiveArray<i64>>()
                    .ok_or_else(|| {
                        format!(
                            "Failed to downcast Date64 array with dtype {:?}",
                            array.dtype()
                        )
                    })?;
                let days = arr.value(row_idx).div_euclid(86_400_000);
                Ok(date_value(days as i32))
            }
            _ => Err(format!("Unsupported date array dtype {:?}", array.dtype())),
        },
        ColumnValueKind::Datetime => {
            let arr = array
                .as_any()
                .downcast_ref::<PrimitiveArray<i64>>()
                .ok_or_else(|| {
                    format!(
                        "Failed to downcast datetime array with dtype {:?}",
                        array.dtype()
                    )
                })?;
            let unit = plan.unit.as_deref().unwrap_or("us");
            if plan.timezone.is_some() {
                Ok(warning_cell(
                    CellValue::String(format_datetime_text(
                        naive_datetime_from_raw(arr.value(row_idx), unit)
                            .ok_or_else(|| "Invalid timezone-aware datetime value.".to_string())?,
                        unit,
                    )),
                    WarningCode::TemporalAsText,
                ))
            } else {
                Ok(datetime_value(arr.value(row_idx), unit))
            }
        }
        ColumnValueKind::Time => {
            let unit = plan.unit.as_deref().unwrap_or("ns");
            let raw = match array.dtype() {
                ArrowDataType::Time32(_) => array
                    .as_any()
                    .downcast_ref::<PrimitiveArray<i32>>()
                    .ok_or_else(|| {
                        format!(
                            "Failed to downcast Time32 array with dtype {:?}",
                            array.dtype()
                        )
                    })?
                    .value(row_idx) as i64,
                ArrowDataType::Time64(_) => array
                    .as_any()
                    .downcast_ref::<PrimitiveArray<i64>>()
                    .ok_or_else(|| {
                        format!(
                            "Failed to downcast Time64 array with dtype {:?}",
                            array.dtype()
                        )
                    })?
                    .value(row_idx),
                _ => return Err(format!("Unsupported time array dtype {:?}", array.dtype())),
            };
            Ok(time_value(raw, unit))
        }
        ColumnValueKind::Duration => {
            let arr = array
                .as_any()
                .downcast_ref::<PrimitiveArray<i64>>()
                .ok_or_else(|| {
                    format!(
                        "Failed to downcast duration array with dtype {:?}",
                        array.dtype()
                    )
                })?;
            Ok(duration_value(
                arr.value(row_idx),
                plan.unit.as_deref().unwrap_or("us"),
            ))
        }
    }
}
