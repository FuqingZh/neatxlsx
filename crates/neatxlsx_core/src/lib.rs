//! `neatxlsx-core` v1:
//! Rust-side XLSX helper kernel.
//!
//! Architecture mirrors Python `io/xlsx` modules:
//! - `conf`   : constants and default presets
//! - `spec`   : specs/models/options
//! - `util`   : pure helper functions
//! - `writer` : pure-Rust writer kernel
pub mod constant;
pub mod spec;
pub mod util;
pub mod writer;

pub use constant::{LEN_SHEET_NAME_MAX, NCOLS_SHEET_MAX, NROWS_SHEET_MAX, SHEET_NAME_ILLEGAL_CHRS};
pub use spec::{
    AutofitMode, AutofitPolicy, CellBorder, CellFormatPatch, IntegerCoerceMode, ScientificPolicy,
    ScientificScope, SheetHorizontalMerge, SheetSlice, XlsxReport, XlsxRowChunkPolicy,
    XlsxValuePolicy, XlsxWriteOptions,
};
pub use util::{
    apply_vertical_run_text_blankout, calculate_row_chunk_size, create_horizontal_merge_tracker,
    derive_contiguous_ranges, plan_horizontal_merges, plan_sheet_slices,
    plan_vertical_visual_merge_borders, sanitize_sheet_name,
};
pub use writer::{XlsxRecordBatch, XlsxRecordBatchResult, XlsxSheetWriteOptions, XlsxWriter};
