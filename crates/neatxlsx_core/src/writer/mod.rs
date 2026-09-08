//! XLSX writer lifecycle and public Rust entrypoints.

mod render;
mod stream;
mod value;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use rust_xlsxwriter::Workbook;

use crate::constant::{ColumnIdentifier, LEN_SHEET_NAME_MAX};
use crate::spec::{
    AutofitPolicy, CellFormatPatch, ColumnValuePlan, ScientificPolicy, XlsxReport, XlsxWriteOptions,
};
use render::format_xlsx_error_text;
pub use stream::{XlsxRecordBatch, XlsxRecordBatchResult};

/// Per-sheet call options (aligned with Python `Workbook.write_sheet` kwargs).
#[derive(Default, Debug, Clone)]
pub struct XlsxSheetWriteOptions {
    /// Optional per-row patches for a custom header; `None` inherits the writer header format.
    pub header_row_formats: Vec<Option<CellFormatPatch>>,
    /// Per-source-column header format patches keyed by zero-based logical index.
    pub header_column_formats: BTreeMap<usize, CellFormatPatch>,
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

impl XlsxSheetWriteOptions {
    /// Reject option combinations that must fail before a worksheet is mutated.
    pub(super) fn validate_preflight(&self) -> Result<(), String> {
        if self.should_merge_header && !self.header_column_formats.is_empty() {
            return Err(
                "header_column_formats cannot be nonempty when merge_header=True.".to_string(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum XlsxWriterState {
    Open,
    Poisoned,
    Closed,
}

/// Test-only finalization seams used to verify poisoning after post-write failures.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TestFinalizeFailurePoint {
    ApplyColumnWidths,
    RenameWorksheets,
    ReorderWorksheets,
    ConstructReport,
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
    state: XlsxWriterState,
    #[cfg(test)]
    test_finalize_failure: Option<TestFinalizeFailurePoint>,
}

impl XlsxWriter {
    /// Create a writer bound to the output path and workbook policies.
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
            state: XlsxWriterState::Open,
            #[cfg(test)]
            test_finalize_failure: None,
        })
    }

    /// Return output file path as string.
    pub fn file_out(&self) -> String {
        self.path_file_out.to_string_lossy().to_string()
    }

    /// Return an immutable snapshot of completed logical-sheet reports.
    pub fn report(&self) -> Vec<XlsxReport> {
        self.reports.clone()
    }

    /// Save the workbook. Repeated successful calls are idempotent.
    pub fn close(&mut self) -> Result<(), String> {
        match self.state {
            XlsxWriterState::Closed => return Ok(()),
            XlsxWriterState::Poisoned => {
                return Err("Cannot close a poisoned workbook.".to_string());
            }
            XlsxWriterState::Open => {}
        }
        if let Err(error) = self
            .workbook
            .save(&self.path_file_out)
            .map_err(format_xlsx_error_text)
        {
            self.state = XlsxWriterState::Poisoned;
            return Err(error);
        }
        self.state = XlsxWriterState::Closed;
        Ok(())
    }

    fn require_open(&self) -> Result<(), String> {
        match self.state {
            XlsxWriterState::Open => Ok(()),
            XlsxWriterState::Poisoned => Err("Cannot use a poisoned workbook.".to_string()),
            XlsxWriterState::Closed => Err("Cannot write after close().".to_string()),
        }
    }

    fn mark_poisoned(&mut self) {
        if matches!(self.state, XlsxWriterState::Open) {
            self.state = XlsxWriterState::Poisoned;
        }
    }

    #[cfg(test)]
    pub(super) fn inject_finalize_failure(&mut self, point: TestFinalizeFailurePoint) {
        self.test_finalize_failure = Some(point);
    }

    #[cfg(test)]
    pub(super) fn fail_at_finalize_point(
        &self,
        point: TestFinalizeFailurePoint,
    ) -> Result<(), String> {
        if self.test_finalize_failure == Some(point) {
            return Err(format!("Injected finalization failure at {point:?}."));
        }
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
