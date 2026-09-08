# Interleaved XLSX Comparison

- Status: `incomplete`
- Timestamp: `2026-09-08T04:16:26.798547+00:00`
- Verdict policy: `display-autofit`
- Acceptance passed: `false`
- Order seed: `20260901`
- Analysis seed: `20260902`

| Scenario | Mode | Samples B/C | Median total ratio | 95% CI | ZIP members | Verdict |
| --- | --- | ---: | ---: | ---: | --- | --- |
| huge_tall_parquet_lazyframe_none | none | 8/8 | 1.0198 | [1.0021, 1.0458] | True | failed |
| huge_tall_parquet_lazyframe_header | header | 0/0 | n/a | n/a | True | inconclusive |
| huge_wide_parquet_lazyframe_body | body | 0/0 | n/a | n/a | True | inconclusive |
| huge_wide_parquet_lazyframe_all | all | 0/0 | n/a | n/a | True | inconclusive |
| huge_wide_parquet_lazyframe_body_full_scan | body | 0/0 | n/a | n/a | True | inconclusive |
| huge_wide_parquet_lazyframe_all_full_scan | all | 0/0 | n/a | n/a | True | inconclusive |

Raw execution records are in `raw.jsonl`; the frozen order is in `planned.json`.
