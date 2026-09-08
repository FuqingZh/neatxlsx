# XLSX Benchmark Record

- Timestamp (UTC): `2026-09-01T04:44:32.454847+00:00`
- Command: `scripts/benchmark_xlsx_writer.py --profile default --repeat 3 --warmup 1 --out-dir /tmp/neatxlsx-v6-benchmark/candidate-final`
- Platform: `Linux-4.18.0-348.7.1.el8_5.x86_64-x86_64-with-glibc2.28`
- Python: `3.13.11`
- Rust backend binary: `/home/fqzhang/project/neatxlsx/.worktrees/single-pass-autofit/python/neatxlsx/_native.abi3.so`
- Validation: `enforce_release_backend + validate sheet1 dimension/rows/cols`
- Package versions:
  - `neatxlsx`: `0.2.0`
  - `polars`: `1.43.1`

| backend | scenario | input | rows | cols | chunk | autofit | repeat | median_s | mean_s | min_s | max_s | stdev_s | mean_size_mb |
| --- | --- | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| rust | narrow_tall_dataframe_header | dataframe | 40000 | 12 | 8192 | True (header) | 3 | 0.844 | 0.838 | 0.819 | 0.852 | 0.017 | 2.93 |
| rust | wide_medium_dataframe_all | dataframe | 10000 | 37 | 8192 | True (all) | 3 | 0.713 | 0.715 | 0.701 | 0.731 | 0.015 | 2.40 |
| rust | narrow_tall_parquet_lazyframe_none | parquet_lazyframe | 40000 | 12 | 8192 | False (none) | 3 | 0.827 | 0.896 | 0.815 | 1.046 | 0.130 | 2.93 |
| rust | narrow_tall_parquet_lazyframe_header | parquet_lazyframe | 40000 | 12 | 8192 | True (header) | 3 | 0.836 | 0.854 | 0.817 | 0.909 | 0.049 | 2.93 |
| rust | wide_medium_parquet_lazyframe_body | parquet_lazyframe | 10000 | 37 | 8192 | True (body) | 3 | 0.783 | 0.789 | 0.767 | 0.817 | 0.025 | 2.40 |
| rust | wide_medium_parquet_lazyframe_all | parquet_lazyframe | 10000 | 37 | 8192 | True (all) | 3 | 0.884 | 0.927 | 0.822 | 1.076 | 0.132 | 2.40 |

## Raw Timings (seconds)

- `rust / narrow_tall_dataframe_header`: `[0.8192634601145983, 0.8436242961324751, 0.8519168049097061]`
- `rust / wide_medium_dataframe_all`: `[0.7128316629678011, 0.7014276091940701, 0.730837328825146]`
- `rust / narrow_tall_parquet_lazyframe_none`: `[1.0455528916791081, 0.8154885391704738, 0.827130266930908]`
- `rust / narrow_tall_parquet_lazyframe_header`: `[0.8172426940873265, 0.9090861133299768, 0.8355783130973577]`
- `rust / wide_medium_parquet_lazyframe_body`: `[0.8169054486788809, 0.7833825531415641, 0.7673588437028229]`
- `rust / wide_medium_parquet_lazyframe_all`: `[0.8224843507632613, 0.8837893940508366, 1.0755373910069466]`
