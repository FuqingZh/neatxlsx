# XLSX Benchmark Record

- Timestamp (UTC): `2026-09-01T04:51:25.586511+00:00`
- Command: `scripts/benchmark_xlsx_writer.py --profile huge --repeat 2 --warmup 1 --out-dir /tmp/neatxlsx-v5-benchmark/huge-run2`
- Platform: `Linux-4.18.0-348.7.1.el8_5.x86_64-x86_64-with-glibc2.28`
- Python: `3.13.11`
- Rust backend binary: `/tmp/neatxlsx-v5-benchmark/archive/python/neatxlsx/_native.abi3.so`
- Validation: `enforce_release_backend + validate sheet1 dimension/rows/cols`
- Package versions:
  - `neatxlsx`: `0.2.0`
  - `polars`: `1.43.1`

| backend | scenario | input | rows | cols | chunk | autofit | repeat | median_s | mean_s | min_s | max_s | stdev_s | mean_size_mb |
| --- | --- | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| rust | huge_tall_dataframe_header | dataframe | 250000 | 15 | 8192 | True (header) | 2 | 6.745 | 6.745 | 6.738 | 6.752 | 0.010 | 22.76 |
| rust | huge_wide_dataframe_all | dataframe | 50000 | 61 | 8192 | True (all) | 2 | 6.417 | 6.417 | 6.188 | 6.646 | 0.324 | 20.47 |
| rust | huge_tall_parquet_lazyframe_none | parquet_lazyframe | 250000 | 15 | 8192 | False (none) | 2 | 6.977 | 6.977 | 6.760 | 7.194 | 0.307 | 22.76 |
| rust | huge_tall_parquet_lazyframe_header | parquet_lazyframe | 250000 | 15 | 8192 | True (header) | 2 | 7.055 | 7.055 | 6.781 | 7.329 | 0.388 | 22.76 |
| rust | huge_wide_parquet_lazyframe_body | parquet_lazyframe | 50000 | 61 | 8192 | True (body) | 2 | 6.567 | 6.567 | 6.530 | 6.604 | 0.052 | 20.47 |
| rust | huge_wide_parquet_lazyframe_all | parquet_lazyframe | 50000 | 61 | 8192 | True (all) | 2 | 6.200 | 6.200 | 6.012 | 6.388 | 0.265 | 20.47 |

## Raw Timings (seconds)

- `rust / huge_tall_dataframe_header`: `[6.752384042833, 6.737706831656396]`
- `rust / huge_wide_dataframe_all`: `[6.187632014043629, 6.646035777870566]`
- `rust / huge_tall_parquet_lazyframe_none`: `[7.194191966671497, 6.75960433203727]`
- `rust / huge_tall_parquet_lazyframe_header`: `[6.780675958842039, 7.328813291154802]`
- `rust / huge_wide_parquet_lazyframe_body`: `[6.6041589276865125, 6.530434765852988]`
- `rust / huge_wide_parquet_lazyframe_all`: `[6.012362895067781, 6.387624761555344]`
