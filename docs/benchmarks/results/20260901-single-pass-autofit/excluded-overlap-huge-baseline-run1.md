# XLSX Benchmark Record

- Timestamp (UTC): `2026-09-01T04:48:02.457751+00:00`
- Command: `scripts/benchmark_xlsx_writer.py --profile huge --repeat 2 --warmup 1 --out-dir /tmp/neatxlsx-v5-benchmark/huge-run1`
- Platform: `Linux-4.18.0-348.7.1.el8_5.x86_64-x86_64-with-glibc2.28`
- Python: `3.13.11`
- Rust backend binary: `/tmp/neatxlsx-v5-benchmark/archive/python/neatxlsx/_native.abi3.so`
- Validation: `enforce_release_backend + validate sheet1 dimension/rows/cols`
- Package versions:
  - `neatxlsx`: `0.2.0`
  - `polars`: `1.43.1`

| backend | scenario | input | rows | cols | chunk | autofit | repeat | median_s | mean_s | min_s | max_s | stdev_s | mean_size_mb |
| --- | --- | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| rust | huge_tall_dataframe_header | dataframe | 250000 | 15 | 8192 | True (header) | 2 | 6.801 | 6.801 | 6.720 | 6.882 | 0.115 | 22.76 |
| rust | huge_wide_dataframe_all | dataframe | 50000 | 61 | 8192 | True (all) | 2 | 6.704 | 6.704 | 6.665 | 6.743 | 0.056 | 20.47 |
| rust | huge_tall_parquet_lazyframe_none | parquet_lazyframe | 250000 | 15 | 8192 | False (none) | 2 | 7.605 | 7.605 | 7.459 | 7.751 | 0.206 | 22.76 |
| rust | huge_tall_parquet_lazyframe_header | parquet_lazyframe | 250000 | 15 | 8192 | True (header) | 2 | 7.367 | 7.367 | 7.343 | 7.391 | 0.034 | 22.76 |
| rust | huge_wide_parquet_lazyframe_body | parquet_lazyframe | 50000 | 61 | 8192 | True (body) | 2 | 6.825 | 6.825 | 6.450 | 7.201 | 0.531 | 20.47 |
| rust | huge_wide_parquet_lazyframe_all | parquet_lazyframe | 50000 | 61 | 8192 | True (all) | 2 | 6.603 | 6.603 | 6.536 | 6.670 | 0.094 | 20.47 |

## Raw Timings (seconds)

- `rust / huge_tall_dataframe_header`: `[6.882475324906409, 6.719914962071925]`
- `rust / huge_wide_dataframe_all`: `[6.743310963269323, 6.664561054203659]`
- `rust / huge_tall_parquet_lazyframe_none`: `[7.750751797109842, 7.459421577863395]`
- `rust / huge_tall_parquet_lazyframe_header`: `[7.343231387902051, 7.390678911935538]`
- `rust / huge_wide_parquet_lazyframe_body`: `[6.450257934164256, 7.200573133770376]`
- `rust / huge_wide_parquet_lazyframe_all`: `[6.53621945111081, 6.669788142200559]`
