# XLSX Benchmark Record

- Timestamp (UTC): `2026-09-01T04:55:23.702141+00:00`
- Command: `scripts/benchmark_xlsx_writer.py --profile huge --repeat 2 --warmup 1 --out-dir /tmp/neatxlsx-v5-benchmark/huge-serial-run1`
- Platform: `Linux-4.18.0-348.7.1.el8_5.x86_64-x86_64-with-glibc2.28`
- Python: `3.13.11`
- Rust backend binary: `/tmp/neatxlsx-v5-benchmark/archive/python/neatxlsx/_native.abi3.so`
- Validation: `enforce_release_backend + validate sheet1 dimension/rows/cols`
- Package versions:
  - `neatxlsx`: `0.2.0`
  - `polars`: `1.43.1`

| backend | scenario | input | rows | cols | chunk | autofit | repeat | median_s | mean_s | min_s | max_s | stdev_s | mean_size_mb |
| --- | --- | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| rust | huge_tall_dataframe_header | dataframe | 250000 | 15 | 8192 | True (header) | 2 | 6.578 | 6.578 | 6.479 | 6.676 | 0.139 | 22.76 |
| rust | huge_wide_dataframe_all | dataframe | 50000 | 61 | 8192 | True (all) | 2 | 6.313 | 6.313 | 6.213 | 6.414 | 0.142 | 20.47 |
| rust | huge_tall_parquet_lazyframe_none | parquet_lazyframe | 250000 | 15 | 8192 | False (none) | 2 | 6.206 | 6.206 | 6.206 | 6.207 | 0.001 | 22.76 |
| rust | huge_tall_parquet_lazyframe_header | parquet_lazyframe | 250000 | 15 | 8192 | True (header) | 2 | 6.968 | 6.968 | 6.613 | 7.323 | 0.501 | 22.76 |
| rust | huge_wide_parquet_lazyframe_body | parquet_lazyframe | 50000 | 61 | 8192 | True (body) | 2 | 6.223 | 6.223 | 6.204 | 6.243 | 0.027 | 20.47 |
| rust | huge_wide_parquet_lazyframe_all | parquet_lazyframe | 50000 | 61 | 8192 | True (all) | 2 | 6.163 | 6.163 | 6.026 | 6.300 | 0.194 | 20.47 |

## Raw Timings (seconds)

- `rust / huge_tall_dataframe_header`: `[6.4793587839230895, 6.675839116796851]`
- `rust / huge_wide_dataframe_all`: `[6.413565305061638, 6.21343392925337]`
- `rust / huge_tall_parquet_lazyframe_none`: `[6.205662092659622, 6.2073031682521105]`
- `rust / huge_tall_parquet_lazyframe_header`: `[6.61335837142542, 7.322514005936682]`
- `rust / huge_wide_parquet_lazyframe_body`: `[6.203708195127547, 6.242563673295081]`
- `rust / huge_wide_parquet_lazyframe_all`: `[6.300160538870841, 6.026094357017428]`
