# XLSX Benchmark Record

- Timestamp (UTC): `2026-09-01T04:48:56.754764+00:00`
- Command: `scripts/benchmark_xlsx_writer.py --profile huge --repeat 2 --warmup 1 --out-dir /tmp/neatxlsx-v6-benchmark/huge-candidate`
- Platform: `Linux-4.18.0-348.7.1.el8_5.x86_64-x86_64-with-glibc2.28`
- Python: `3.13.11`
- Rust backend binary: `/home/fqzhang/project/neatxlsx/.worktrees/single-pass-autofit/python/neatxlsx/_native.abi3.so`
- Validation: `enforce_release_backend + validate sheet1 dimension/rows/cols`
- Package versions:
  - `neatxlsx`: `0.2.0`
  - `polars`: `1.43.1`

| backend | scenario | input | rows | cols | chunk | autofit | repeat | median_s | mean_s | min_s | max_s | stdev_s | mean_size_mb |
| --- | --- | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| rust | huge_tall_dataframe_header | dataframe | 250000 | 15 | 8192 | True (header) | 2 | 7.187 | 7.187 | 7.155 | 7.220 | 0.046 | 22.76 |
| rust | huge_wide_dataframe_all | dataframe | 50000 | 61 | 8192 | True (all) | 2 | 6.414 | 6.414 | 6.235 | 6.594 | 0.253 | 20.47 |
| rust | huge_tall_parquet_lazyframe_none | parquet_lazyframe | 250000 | 15 | 8192 | False (none) | 2 | 6.871 | 6.871 | 6.594 | 7.147 | 0.391 | 22.76 |
| rust | huge_tall_parquet_lazyframe_header | parquet_lazyframe | 250000 | 15 | 8192 | True (header) | 2 | 6.836 | 6.836 | 6.795 | 6.877 | 0.058 | 22.76 |
| rust | huge_wide_parquet_lazyframe_body | parquet_lazyframe | 50000 | 61 | 8192 | True (body) | 2 | 6.551 | 6.551 | 6.480 | 6.623 | 0.101 | 20.47 |
| rust | huge_wide_parquet_lazyframe_all | parquet_lazyframe | 50000 | 61 | 8192 | True (all) | 2 | 6.090 | 6.090 | 6.009 | 6.170 | 0.114 | 20.47 |

## Raw Timings (seconds)

- `rust / huge_tall_dataframe_header`: `[7.220030601602048, 7.154767714440823]`
- `rust / huge_wide_dataframe_all`: `[6.593549067154527, 6.235105855856091]`
- `rust / huge_tall_parquet_lazyframe_none`: `[7.147418012842536, 6.5941849229857326]`
- `rust / huge_tall_parquet_lazyframe_header`: `[6.877151898108423, 6.795063031837344]`
- `rust / huge_wide_parquet_lazyframe_body`: `[6.6226774249225855, 6.4798604981042445]`
- `rust / huge_wide_parquet_lazyframe_all`: `[6.009139736648649, 6.170497971121222]`
