# XLSX Benchmark Record

- Timestamp (UTC): `2026-09-01T05:00:58.678094+00:00`
- Command: `scripts/benchmark_xlsx_writer.py --profile huge --repeat 2 --warmup 1 --out-dir /tmp/neatxlsx-v6-benchmark/huge-serial-after-baseline`
- Platform: `Linux-4.18.0-348.7.1.el8_5.x86_64-x86_64-with-glibc2.28`
- Python: `3.13.11`
- Rust backend binary: `/home/fqzhang/project/neatxlsx/.worktrees/single-pass-autofit/python/neatxlsx/_native.abi3.so`
- Validation: `enforce_release_backend + validate sheet1 dimension/rows/cols`
- Package versions:
  - `neatxlsx`: `0.2.0`
  - `polars`: `1.43.1`

| backend | scenario | input | rows | cols | chunk | autofit | repeat | median_s | mean_s | min_s | max_s | stdev_s | mean_size_mb |
| --- | --- | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| rust | huge_tall_dataframe_header | dataframe | 250000 | 15 | 8192 | True (header) | 2 | 6.791 | 6.791 | 6.625 | 6.957 | 0.234 | 22.76 |
| rust | huge_wide_dataframe_all | dataframe | 50000 | 61 | 8192 | True (all) | 2 | 6.118 | 6.118 | 5.965 | 6.271 | 0.216 | 20.47 |
| rust | huge_tall_parquet_lazyframe_none | parquet_lazyframe | 250000 | 15 | 8192 | False (none) | 2 | 6.898 | 6.898 | 6.837 | 6.958 | 0.086 | 22.76 |
| rust | huge_tall_parquet_lazyframe_header | parquet_lazyframe | 250000 | 15 | 8192 | True (header) | 2 | 6.524 | 6.524 | 6.289 | 6.759 | 0.332 | 22.76 |
| rust | huge_wide_parquet_lazyframe_body | parquet_lazyframe | 50000 | 61 | 8192 | True (body) | 2 | 5.936 | 5.936 | 5.791 | 6.082 | 0.206 | 20.47 |
| rust | huge_wide_parquet_lazyframe_all | parquet_lazyframe | 50000 | 61 | 8192 | True (all) | 2 | 6.287 | 6.287 | 6.152 | 6.421 | 0.191 | 20.47 |

## Raw Timings (seconds)

- `rust / huge_tall_dataframe_header`: `[6.956540498882532, 6.624965820927173]`
- `rust / huge_wide_dataframe_all`: `[5.96530078491196, 6.270909247919917]`
- `rust / huge_tall_parquet_lazyframe_none`: `[6.958400153089315, 6.837008616887033]`
- `rust / huge_tall_parquet_lazyframe_header`: `[6.758598908782005, 6.289432427380234]`
- `rust / huge_wide_parquet_lazyframe_body`: `[6.082371285185218, 5.790534591302276]`
- `rust / huge_wide_parquet_lazyframe_all`: `[6.151878792792559, 6.421408046968281]`
