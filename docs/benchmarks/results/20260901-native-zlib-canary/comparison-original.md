# Interleaved XLSX Comparison

- Status: `complete`
- Timestamp: `2026-09-01T06:15:30.436948+00:00`
- Verdict policy: `zlib`
- Order seed: `20260901`
- Analysis seed: `20260902`

| Scenario | Mode | Samples B/C | Median total ratio | 95% CI | ZIP members | Verdict |
| --- | --- | ---: | ---: | ---: | --- | --- |
| huge_tall_parquet_lazyframe_none | none | 8/8 | 1.2189 | [1.1262, 1.2679] | True | inconclusive |
| huge_tall_parquet_lazyframe_header | header | 8/8 | 1.2977 | [1.2041, 1.3592] | True | inconclusive |
| huge_wide_parquet_lazyframe_body | body | 8/8 | 1.1676 | [1.1222, 1.2010] | True | inconclusive |
| huge_wide_parquet_lazyframe_all | all | 8/8 | 1.1608 | [1.1345, 1.1954] | True | inconclusive |
| huge_wide_parquet_lazyframe_all_full_scan | all | 8/8 | 1.1657 | [1.1494, 1.1800] | True | inconclusive |

Raw execution records are in `raw.jsonl`; the frozen order is in `planned.json`.
