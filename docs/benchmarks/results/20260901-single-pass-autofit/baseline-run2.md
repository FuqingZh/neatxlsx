# XLSX Benchmark Record

- Timestamp (UTC): `2026-09-01T04:43:28.918696+00:00`
- Command: `scripts/benchmark_xlsx_writer.py --profile default --repeat 3 --warmup 1 --out-dir /tmp/neatxlsx-v5-benchmark/run2`
- Platform: `Linux-4.18.0-348.7.1.el8_5.x86_64-x86_64-with-glibc2.28`
- Python: `3.13.11`
- Rust backend binary: `/tmp/neatxlsx-v5-benchmark/archive/python/neatxlsx/_native.abi3.so`
- Validation: `enforce_release_backend + validate sheet1 dimension/rows/cols`
- Package versions:
  - `neatxlsx`: `0.2.0`
  - `polars`: `1.43.1`

| backend | scenario | input | rows | cols | chunk | autofit | repeat | median_s | mean_s | min_s | max_s | stdev_s | mean_size_mb |
| --- | --- | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| rust | narrow_tall_dataframe_header | dataframe | 40000 | 12 | 8192 | True (header) | 3 | 0.911 | 0.904 | 0.889 | 0.913 | 0.013 | 2.93 |
| rust | wide_medium_dataframe_all | dataframe | 10000 | 37 | 8192 | True (all) | 3 | 0.844 | 0.845 | 0.837 | 0.853 | 0.008 | 2.40 |
| rust | narrow_tall_parquet_lazyframe_none | parquet_lazyframe | 40000 | 12 | 8192 | False (none) | 3 | 0.918 | 0.904 | 0.853 | 0.940 | 0.045 | 2.93 |
| rust | narrow_tall_parquet_lazyframe_header | parquet_lazyframe | 40000 | 12 | 8192 | True (header) | 3 | 0.931 | 0.956 | 0.925 | 1.011 | 0.048 | 2.93 |
| rust | wide_medium_parquet_lazyframe_body | parquet_lazyframe | 10000 | 37 | 8192 | True (body) | 3 | 0.829 | 0.835 | 0.810 | 0.867 | 0.029 | 2.40 |
| rust | wide_medium_parquet_lazyframe_all | parquet_lazyframe | 10000 | 37 | 8192 | True (all) | 3 | 0.905 | 0.897 | 0.874 | 0.912 | 0.020 | 2.40 |

## Raw Timings (seconds)

- `rust / narrow_tall_dataframe_header`: `[0.9129068031907082, 0.9110034005716443, 0.8891322962008417]`
- `rust / wide_medium_dataframe_all`: `[0.8440024149604142, 0.8528278400190175, 0.836691590026021]`
- `rust / narrow_tall_parquet_lazyframe_none`: `[0.8532826858572662, 0.9184347791597247, 0.9395569092594087]`
- `rust / narrow_tall_parquet_lazyframe_header`: `[0.930809497833252, 0.9253095556050539, 1.0113829411566257]`
- `rust / wide_medium_parquet_lazyframe_body`: `[0.8285979791544378, 0.8101913421414793, 0.8666984713636339]`
- `rust / wide_medium_parquet_lazyframe_all`: `[0.9118731319904327, 0.9049852839671075, 0.873750934842974]`
