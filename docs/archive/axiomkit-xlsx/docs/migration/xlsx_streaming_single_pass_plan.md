# XLSX Streaming Single-Pass Plan

## Summary

Optimize the XLSX streaming writer for large Arrow/Polars inputs by adding a
single-pass path for `AutofitPolicy(mode="header" | "none")` and by writing
Arrow record batches directly instead of converting each batch into a temporary
Polars `DataFrame`.

## Key Changes

- Keep `XlsxWriter.write_sheet(...)` as the public Python API.
- Add a Rust core single-pass batch writer used only when body autofit is not
  required.
- Use the first Arrow record batch to resolve schema, header grid, column
  formats, freeze panes, and header-only widths, then write it immediately.
- Dynamically create additional worksheets as row/column Excel limits are
  reached instead of requiring a precomputed total row count.
- Replace the streaming path's `RecordBatch -> DataFrame -> AnyValue` write
  loop with Arrow-array typed value extraction.
- Keep the existing two-pass planner for `autofit=body/all` and as fallback
  for behavior that needs body pre-scan.

## Constraints

- No public API breakage.
- Existing formatting semantics for headers, merges, freeze panes, missing
  values, numeric/integer/decimal/scientific formats, and sheet slicing must be
  preserved.
- Single-pass mode must still report final `SheetSlice` row ranges after write.
- Empty streams must keep the current clear error semantics.

## Tests

- Existing XLSX semantic and smoke tests must pass.
- Add/extend tests to cover LazyFrame single-pass output parity for
  `autofit=header` and `autofit=none`.
- Keep tests for `autofit=body/all` on the existing two-pass path.
- Re-run large smoke benchmarks for `2000 x 1400 x 2` and
  `10000 x 1400 x 2`, recording elapsed time, XLSX size, and peak RSS.
