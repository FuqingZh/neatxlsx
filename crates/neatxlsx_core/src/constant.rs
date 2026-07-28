//! XLSX constants and default preset factories.

use std::collections::BTreeMap;

use crate::spec::{CellFormatPatch, XlsxWriteOptions};

/// Excel worksheet maximum row count.
pub const NROWS_SHEET_MAX: usize = 1_048_576;
/// Excel worksheet maximum column count.
pub const NCOLS_SHEET_MAX: usize = 16_384;
/// Excel sheet name maximum length.
pub const LEN_SHEET_NAME_MAX: usize = 31;
/// Characters not allowed in sheet names.
pub const SHEET_NAME_ILLEGAL_CHRS: [&str; 7] = ["*", ":", "?", "/", "\\", "[", "]"];

/// Canonical format preset keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatKey {
    /// Generic text cell format.
    Text,
    /// Integer number format.
    Integer,
    /// Decimal number format.
    Decimal,
    /// Scientific number format.
    Scientific,
    /// Header cell format.
    Header,
}

/// Column selector reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnIdentifier {
    /// Select by column name.
    Name(String),
    /// Select by zero-based column index.
    Index(usize),
}

/// Build default named format presets used by [`crate::writer::XlsxWriter`].
pub fn create_default_xlsx_formats() -> BTreeMap<String, CellFormatPatch> {
    let base_format_spec = CellFormatPatch {
        font_name: Some("Times New Roman".to_string()),
        font_size: Some(11),
        border: Some(1),
        align: Some("left".to_string()),
        valign: Some("vcenter".to_string()),
        ..Default::default()
    };

    let mut formats = BTreeMap::new();
    formats.insert("text".to_string(), base_format_spec.clone());
    formats.insert(
        "header".to_string(),
        base_format_spec.with_(CellFormatPatch {
            bold: Some(true),
            align: Some("center".to_string()),
            ..Default::default()
        }),
    );
    formats.insert(
        "integer".to_string(),
        base_format_spec.with_(CellFormatPatch {
            num_format: Some("0".to_string()),
            ..Default::default()
        }),
    );
    formats.insert(
        "decimal".to_string(),
        base_format_spec.with_(CellFormatPatch {
            num_format: Some("0.0000".to_string()),
            ..Default::default()
        }),
    );
    formats.insert(
        "scientific".to_string(),
        base_format_spec.with_(CellFormatPatch {
            num_format: Some("0.00E+0".to_string()),
            ..Default::default()
        }),
    );

    formats
}

/// Build default write options.
pub fn create_default_xlsx_write_options() -> XlsxWriteOptions {
    XlsxWriteOptions::default()
}
