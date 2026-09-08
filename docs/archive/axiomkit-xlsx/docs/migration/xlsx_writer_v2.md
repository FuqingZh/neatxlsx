# XLSX Writer v2 Migration Note

This release hard-renames the Python/Rust XLSX API and bridge contract.

Bridge changes:
- `__bridge_abi__`: `1` -> `2`
- `__bridge_contract__`: `axiomkit.xlsx.writer.v1` -> `axiomkit.xlsx.writer.v2`

Public Python rename map:
- `CellFormatSpec` -> `CellFormatPatch`
- `CellBorderSpec` -> `CellBorder`
- `XlsxValuePolicySpec` -> `XlsxValuePolicy`
- `XlsxRowChunkPolicySpec` -> `XlsxRowChunkPolicy`
- `XlsxWriteOptionsSpec` -> `XlsxWriteOptions`
- `ScientificPolicySpec` -> `ScientificPolicy`
- `AutofitCellsPolicySpec` -> `AutofitPolicy`
- `SheetSliceSpec` -> `SheetSlice`
- `SheetHorizontalMergeSpec` -> `SheetHorizontalMerge`
- `write_options=` -> `options_write=`
- `AutofitPolicy(rule_columns=...)` -> `AutofitPolicy(mode=...)`
- `ScientificPolicy(rule_scope=...)` -> `ScientificPolicy(scope=...)`

Rust rename map:
- `CellFormatSpec` -> `CellFormatPatch`
- `CellBorderSpec` -> `CellBorder`
- `XlsxValuePolicySpec` -> `XlsxValuePolicy`
- `XlsxRowChunkPolicySpec` -> `XlsxRowChunkPolicy`
- `XlsxWriteOptionsSpec` -> `XlsxWriteOptions`
- `ScientificPolicySpec` -> `ScientificPolicy`
- `AutofitCellsPolicySpec` -> `AutofitPolicy`
- `SheetSliceSpec` -> `SheetSlice`
- `SheetHorizontalMergeSpec` -> `SheetHorizontalMerge`
- `ColumnFormatPlanSpec` -> `ColumnFormatPlan`
- `XlsxSheetWriteOptionsSpec` -> `XlsxSheetWriteOptions`
- `AutofitColumnsRule` -> `AutofitMode`

Behavior changes:
- `ScientificPolicy` default now disables scientific formatting. To enable,
  pass `ScientificPolicy(scope="decimal")` (or other scopes) explicitly.
- `ScientificPolicy.height_body_inferred_max` has been removed (no longer used in
  per-cell scientific formatting).
