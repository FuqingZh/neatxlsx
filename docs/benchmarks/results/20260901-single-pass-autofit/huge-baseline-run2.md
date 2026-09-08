# XLSX Benchmark Record

- Timestamp (UTC): `2026-09-01T04:57:44.965887+00:00`
- Command: `scripts/benchmark_xlsx_writer.py --profile huge --repeat 2 --warmup 1 --out-dir /tmp/neatxlsx-v5-benchmark/huge-serial-run2`
- Platform: `Linux-4.18.0-348.7.1.el8_5.x86_64-x86_64-with-glibc2.28`
- Python: `3.13.11`
- Rust backend binary: `/tmp/neatxlsx-v5-benchmark/archive/python/neatxlsx/_native.abi3.so`
- Validation: `enforce_release_backend + validate sheet1 dimension/rows/cols`
- Package versions:
  - `neatxlsx`: `0.2.0`
  - `polars`: `1.43.1`

| backend | scenario | input | rows | cols | chunk | autofit | repeat | median_s | mean_s | min_s | max_s | stdev_s | mean_size_mb |
| --- | --- | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| rust | huge_tall_dataframe_header | dataframe | 250000 | 15 | 8192 | True (header) | 2 | 6.693 | 6.693 | 6.604 | 6.782 | 0.126 | 22.76 |
| rust | huge_wide_dataframe_all | dataframe | 50000 | 61 | 8192 | True (all) | 2 | 6.478 | 6.478 | 6.317 | 6.639 | 0.228 | 20.47 |
| rust | huge_tall_parquet_lazyframe_none | parquet_lazyframe | 250000 | 15 | 8192 | False (none) | 2 | 6.247 | 6.247 | 6.034 | 6.460 | 0.301 | 22.76 |
| rust | huge_tall_parquet_lazyframe_header | parquet_lazyframe | 250000 | 15 | 8192 | True (header) | 2 | 6.353 | 6.353 | 6.327 | 6.378 | 0.036 | 22.76 |
| rust | huge_wide_parquet_lazyframe_body | parquet_lazyframe | 50000 | 61 | 8192 | True (body) | 2 | 6.427 | 6.427 | 6.403 | 6.451 | 0.034 | 20.47 |
| rust | huge_wide_parquet_lazyframe_all | parquet_lazyframe | 50000 | 61 | 8192 | True (all) | 2 | 6.533 | 6.533 | 6.408 | 6.658 | 0.177 | 20.47 |

## Raw Timings (seconds)

- `rust / huge_tall_dataframe_header`: `[6.781962262000889, 6.604328787885606]`
- `rust / huge_wide_dataframe_all`: `[6.316904047969729, 6.638882991857827]`
- `rust / huge_tall_parquet_lazyframe_none`: `[6.460087529849261, 6.033983143046498]`
- `rust / huge_tall_parquet_lazyframe_header`: `[6.327087479177862, 6.378432299941778]`
- `rust / huge_wide_parquet_lazyframe_body`: `[6.4512334410101175, 6.403012012131512]`
- `rust / huge_wide_parquet_lazyframe_all`: `[6.6581933638080955, 6.408490544185042]`
