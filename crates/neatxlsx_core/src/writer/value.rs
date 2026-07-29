//! DataFrame and Arrow value normalization for XLSX planning and rendering.

use std::io::Cursor;

use arrow::array::{
    Array as ArrowArray, BooleanArray, PrimitiveArray, TryExtend, Utf8Array, Utf8ViewArray,
};
use arrow::datatypes::{ArrowDataType, ArrowSchema};
use polars::prelude::{AnyValue, DataFrame, IpcReader, SerReader};

use crate::spec::{AutofitPolicy, CellValue, ScientificPolicy, ScientificScope, XlsxValuePolicy};

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

pub(super) fn convert_arrow_value_to_cell_value(
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
