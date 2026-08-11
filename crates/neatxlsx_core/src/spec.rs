//! Shared XLSX specification models.

use std::collections::BTreeMap;

////////////////////////////////////////////////////////////////////////////////
// #region CellFormatSpecification

/// Cell format patch aligned with Python `CellFormatPatch`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct CellFormatPatch {
    /// Font family name.
    pub font_name: Option<String>,
    /// Font size in points.
    pub font_size: Option<i64>,
    /// Bold style.
    pub bold: Option<bool>,
    /// Italic style.
    pub italic: Option<bool>,

    /// Horizontal alignment.
    pub align: Option<String>,
    /// Vertical alignment.
    pub valign: Option<String>,
    /// Border style for all sides.
    pub border: Option<i64>,
    /// Text wrap.
    pub text_wrap: Option<bool>,

    /// Top border override.
    pub top: Option<i64>,
    /// Bottom border override.
    pub bottom: Option<i64>,
    /// Left border override.
    pub left: Option<i64>,
    /// Right border override.
    pub right: Option<i64>,

    /// Number format code.
    pub num_format: Option<String>,
    /// Background fill color.
    pub bg_color: Option<String>,
    /// Font color.
    pub font_color: Option<String>,
}

/// Scalar value for generic format-map representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellFormatValue {
    /// String format property value.
    String(String),
    /// Integer format property value.
    Integer(i64),
    /// Boolean format property value.
    Boolean(bool),
}

/// Normalized cell value during conversion/write pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    /// Missing/blank value.
    Blank,
    /// Text value.
    String(String),
    /// Numeric value.
    Number(f64),
    /// Native boolean value.
    Boolean(bool),
}

/// Logical value kind selected by the Python Polars preflight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnValueKind {
    /// Null-only column.
    Null,
    /// Native boolean column.
    Boolean,
    /// Signed or unsigned integer column.
    Integer,
    /// Floating-point column.
    Float,
    /// Fixed-scale decimal column.
    Decimal,
    /// Literal string column.
    String,
    /// Excel date column.
    Date,
    /// Timezone-naive datetime column.
    Datetime,
    /// Time-of-day column.
    Time,
    /// Duration column.
    Duration,
}

/// Ordered source-column metadata transported over the private bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnValuePlan {
    /// Source column name.
    pub name: String,
    /// Logical value kind.
    pub kind: ColumnValueKind,
    /// Source temporal unit (`day`, `s`, `ms`, `us`, or `ns`).
    pub unit: Option<String>,
    /// Source timezone, when present.
    pub timezone: Option<String>,
    /// Decimal precision, when applicable.
    pub decimal_precision: Option<usize>,
    /// Decimal scale, when applicable.
    pub decimal_scale: Option<usize>,
}

/// Non-fatal conversion warning category used for logical-sheet aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningCode {
    /// Exact text was required to avoid numeric precision loss.
    PrecisionAsText,
    /// Exact text was required because Excel's temporal serial is lossy.
    TemporalAsText,
}

impl CellFormatPatch {
    /// Return a new format by overlaying `patch` onto `self`.
    pub fn with_(&self, patch: CellFormatPatch) -> CellFormatPatch {
        self.merge(&patch)
    }

    /// Merge two formats with right-side non-`None` overwrite semantics.
    pub fn merge(&self, other: &CellFormatPatch) -> CellFormatPatch {
        CellFormatPatch {
            font_name: other.font_name.clone().or_else(|| self.font_name.clone()),
            font_size: other.font_size.or(self.font_size),
            bold: other.bold.or(self.bold),
            italic: other.italic.or(self.italic),
            align: other.align.clone().or_else(|| self.align.clone()),
            valign: other.valign.clone().or_else(|| self.valign.clone()),
            border: other.border.or(self.border),
            text_wrap: other.text_wrap.or(self.text_wrap),
            top: other.top.or(self.top),
            bottom: other.bottom.or(self.bottom),
            left: other.left.or(self.left),
            right: other.right.or(self.right),
            num_format: other.num_format.clone().or_else(|| self.num_format.clone()),
            bg_color: other.bg_color.clone().or_else(|| self.bg_color.clone()),
            font_color: other.font_color.clone().or_else(|| self.font_color.clone()),
        }
    }

    /// Convert format into key-value map compatible with xlsxwriter properties.
    pub fn to_xlsxwriter(&self) -> BTreeMap<String, CellFormatValue> {
        let mut format_map = BTreeMap::new();

        if let Some(value) = &self.font_name {
            format_map.insert(
                "font_name".to_string(),
                CellFormatValue::String(value.clone()),
            );
        }
        if let Some(value) = self.font_size {
            format_map.insert("font_size".to_string(), CellFormatValue::Integer(value));
        }
        if let Some(value) = self.bold {
            format_map.insert("bold".to_string(), CellFormatValue::Boolean(value));
        }
        if let Some(value) = self.italic {
            format_map.insert("italic".to_string(), CellFormatValue::Boolean(value));
        }

        if let Some(value) = &self.align {
            format_map.insert("align".to_string(), CellFormatValue::String(value.clone()));
        }
        if let Some(value) = &self.valign {
            format_map.insert("valign".to_string(), CellFormatValue::String(value.clone()));
        }
        if let Some(value) = self.border {
            format_map.insert("border".to_string(), CellFormatValue::Integer(value));
        }
        if let Some(value) = self.text_wrap {
            format_map.insert("text_wrap".to_string(), CellFormatValue::Boolean(value));
        }

        if let Some(value) = self.top {
            format_map.insert("top".to_string(), CellFormatValue::Integer(value));
        }
        if let Some(value) = self.bottom {
            format_map.insert("bottom".to_string(), CellFormatValue::Integer(value));
        }
        if let Some(value) = self.left {
            format_map.insert("left".to_string(), CellFormatValue::Integer(value));
        }
        if let Some(value) = self.right {
            format_map.insert("right".to_string(), CellFormatValue::Integer(value));
        }

        if let Some(value) = &self.num_format {
            format_map.insert(
                "num_format".to_string(),
                CellFormatValue::String(value.clone()),
            );
        }
        if let Some(value) = &self.bg_color {
            format_map.insert(
                "bg_color".to_string(),
                CellFormatValue::String(value.clone()),
            );
        }
        if let Some(value) = &self.font_color {
            format_map.insert(
                "font_color".to_string(),
                CellFormatValue::String(value.clone()),
            );
        }

        format_map
    }
}

/// Border tuple for top/bottom/left/right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellBorder {
    /// Top border style.
    pub top: i64,
    /// Bottom border style.
    pub bottom: i64,
    /// Left border style.
    pub left: i64,
    /// Right border style.
    pub right: i64,
}

// #endregion
////////////////////////////////////////////////////////////////////////////////
// #region ColumnFormatSpecification

/// Planned final/base formats by column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnFormatPlan {
    /// Final format applied at write time.
    pub fmts_by_col: Vec<CellFormatPatch>,
    /// Base format before per-column override.
    pub fmts_base_by_col: Vec<CellFormatPatch>,
}

// #endregion
////////////////////////////////////////////////////////////////////////////////
// #region WriteOptions

/// Integer conversion policy for numeric-looking values in integer columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IntegerCoerceMode {
    /// Coerce numeric values to integer representation when possible.
    Coerce,
    /// Keep non-integer numeric values as text in integer columns.
    #[default]
    Strict,
}

/// Value conversion policy for missing/NaN/Inf and integer coercion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsxValuePolicy {
    /// Replacement text for missing value when keep-missing is enabled.
    pub missing_value_str: String,
    /// Replacement text for NaN.
    pub nan_str: String,
    /// Replacement text for positive infinity.
    pub posinf_str: String,
    /// Replacement text for negative infinity.
    pub neginf_str: String,
    /// Integer conversion mode.
    pub integer_coerce: IntegerCoerceMode,
}

impl Default for XlsxValuePolicy {
    fn default() -> Self {
        Self {
            missing_value_str: "NA".to_string(),
            nan_str: "NaN".to_string(),
            posinf_str: "Inf".to_string(),
            neginf_str: "-Inf".to_string(),
            integer_coerce: IntegerCoerceMode::Strict,
        }
    }
}

/// Policy for selecting row chunk size in write pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsxRowChunkPolicy {
    /// Width threshold for large table.
    pub width_large: usize,
    /// Width threshold for medium table.
    pub width_medium: usize,
    /// Chunk size used when width >= `width_large`.
    pub size_large: usize,
    /// Chunk size used when width >= `width_medium`.
    pub size_medium: usize,
    /// Default chunk size.
    pub size_default: usize,
    /// Force exact chunk size when set.
    pub fixed_size: Option<usize>,
}

impl Default for XlsxRowChunkPolicy {
    fn default() -> Self {
        Self {
            width_large: 8_000,
            width_medium: 2_000,
            size_large: 1_000,
            size_medium: 2_000,
            size_default: 10_000,
            fixed_size: None,
        }
    }
}

/// Scientific formatting candidate scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScientificScope {
    /// Disable scientific auto-selection entirely.
    #[default]
    None,
    /// Apply to decimal-like numeric columns.
    Decimal,
    /// Apply to integer columns.
    Integer,
    /// Apply to all numeric columns.
    All,
}

/// Scientific formatting policy used in per-sheet write.
#[derive(Debug, Clone, PartialEq)]
pub struct ScientificPolicy {
    /// Scientific trigger scope.
    pub scope: ScientificScope,
    /// Lower absolute bound trigger (exclusive, except zero).
    pub thr_min: f64,
    /// Upper absolute bound trigger (inclusive).
    pub thr_max: f64,
}

impl Default for ScientificPolicy {
    fn default() -> Self {
        Self {
            scope: ScientificScope::None,
            thr_min: 0.0001,
            thr_max: 1_000_000_000_000.0,
        }
    }
}

/// Autofit rule for column width inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutofitMode {
    /// Disable autofit.
    None,
    /// Infer width from header cells only (default).
    #[default]
    Header,
    /// Infer width from body cells only.
    Body,
    /// Infer width from both header and body cells.
    All,
}

/// Autofit policy for per-sheet write call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutofitPolicy {
    /// Autofit width inference rule.
    pub mode: AutofitMode,
    /// Max body rows inspected when body-based inference is active.
    pub height_body_inferred_max: Option<usize>,
    /// Minimum final width.
    pub width_cell_min: usize,
    /// Maximum final width.
    pub width_cell_max: usize,
    /// Width padding added after inference.
    pub width_cell_padding: usize,
}

impl Default for AutofitPolicy {
    fn default() -> Self {
        Self {
            mode: AutofitMode::Header,
            height_body_inferred_max: Some(20_000),
            width_cell_min: 8,
            width_cell_max: 60,
            width_cell_padding: 2,
        }
    }
}

/// Writer-wide options controlling value conversion and formatting defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsxWriteOptions {
    /// Value conversion policy.
    pub value_policy: XlsxValuePolicy,
    /// Use ZIP64 extensions for the XLSX container.
    pub should_use_zip64: bool,
    /// Keep missing/NaN/Inf as text instead of blank.
    pub should_keep_missing_values: bool,
    /// Infer numeric columns from dtypes.
    pub should_infer_numeric_cols: bool,
    /// Infer integer subset from numeric columns.
    pub should_infer_integer_cols: bool,
    /// Row chunking policy.
    pub row_chunk_policy: XlsxRowChunkPolicy,
}

impl Default for XlsxWriteOptions {
    fn default() -> Self {
        Self {
            value_policy: XlsxValuePolicy::default(),
            should_use_zip64: true,
            should_keep_missing_values: false,
            should_infer_numeric_cols: true,
            should_infer_integer_cols: true,
            row_chunk_policy: XlsxRowChunkPolicy::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::XlsxWriteOptions;

    #[test]
    fn xlsx_write_options_enable_zip64_by_default() {
        assert!(XlsxWriteOptions::default().should_use_zip64);
    }
}

// #endregion
////////////////////////////////////////////////////////////////////////////////
// #region SheetFormatSpecification

/// Concrete sheet part emitted to workbook (after Excel-limit slicing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetSlice {
    /// Actual unique sheet name in workbook.
    pub sheet_name: String,
    /// Inclusive source row start.
    pub row_start_inclusive: usize,
    /// Exclusive source row end.
    pub row_end_exclusive: usize,
    /// Inclusive source column start.
    pub col_start_inclusive: usize,
    /// Exclusive source column end.
    pub col_end_exclusive: usize,
}

/// Horizontal merge plan item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetHorizontalMerge {
    /// Row index where merge is applied.
    pub row_idx_start: usize,
    /// Start column index (inclusive).
    pub col_idx_start: usize,
    /// End column index (inclusive).
    pub col_idx_end: usize,
    /// Merge display text.
    pub text: String,
}

// #endregion
////////////////////////////////////////////////////////////////////////////////
// #region ReportSpecification

/// Per-write call report.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct XlsxReport {
    /// Sheet slices produced by the write call.
    pub sheets: Vec<SheetSlice>,
    /// Non-fatal warnings.
    pub warnings: Vec<String>,
}

impl XlsxReport {
    /// Add a warning message.
    pub fn warn(&mut self, msg: impl AsRef<str>) {
        self.warnings.push(msg.as_ref().to_string());
    }
}

// #endregion
////////////////////////////////////////////////////////////////////////////////
