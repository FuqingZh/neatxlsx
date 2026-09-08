# Interleaved XLSX Comparison

- Status: `complete`
- Timestamp: `2026-09-01T11:50:51.692590+00:00`
- Verdict policy: `display-autofit`
- Order seed: `20260901`
- Analysis seed: `20260902`

| Scenario | Mode | Samples B/C | Median total ratio | 95% CI | ZIP members | Verdict |
| --- | --- | ---: | ---: | ---: | --- | --- |
| huge_tall_parquet_lazyframe_none | none | 8/8 | 1.0002 | [0.9729, 1.0488] | True | failed |
| huge_tall_parquet_lazyframe_header | header | 8/8 | 1.0009 | [0.9530, 1.0333] | True | failed |
| huge_wide_parquet_lazyframe_body | body | 8/8 | 1.0066 | [0.9723, 1.0561] | True | failed |
| huge_wide_parquet_lazyframe_all | all | 8/8 | 1.0328 | [0.9854, 1.0724] | True | failed |
| huge_wide_parquet_lazyframe_body_full_scan | body | 8/8 | 1.0731 | [1.0254, 1.1539] | True | failed |
| huge_wide_parquet_lazyframe_all_full_scan | all | 8/8 | 1.0483 | [1.0266, 1.0831] | True | passed |

Raw execution records are in `raw.jsonl`; the frozen order is in `planned.json`.
