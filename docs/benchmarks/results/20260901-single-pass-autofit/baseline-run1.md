# XLSX Benchmark Record

- Timestamp (UTC): `2026-09-01T04:42:53.760112+00:00`
- Command: `scripts/benchmark_xlsx_writer.py --profile default --repeat 3 --warmup 1 --out-dir /tmp/neatxlsx-v5-benchmark/run1`
- Platform: `Linux-4.18.0-348.7.1.el8_5.x86_64-x86_64-with-glibc2.28`
- Python: `3.13.11`
- Rust backend binary: `/tmp/neatxlsx-v5-benchmark/archive/python/neatxlsx/_native.abi3.so`
- Validation: `enforce_release_backend + validate sheet1 dimension/rows/cols`
- Package versions:
  - `neatxlsx`: `0.2.0`
  - `polars`: `1.43.1`

| backend | scenario | input | rows | cols | chunk | autofit | repeat | median_s | mean_s | min_s | max_s | stdev_s | mean_size_mb |
| --- | --- | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| rust | narrow_tall_dataframe_header | dataframe | 40000 | 12 | 8192 | True (header) | 3 | 0.854 | 0.894 | 0.851 | 0.975 | 0.071 | 2.93 |
| rust | wide_medium_dataframe_all | dataframe | 10000 | 37 | 8192 | True (all) | 3 | 1.083 | 1.008 | 0.810 | 1.130 | 0.173 | 2.40 |
| rust | narrow_tall_parquet_lazyframe_none | parquet_lazyframe | 40000 | 12 | 8192 | False (none) | 3 | 0.896 | 0.883 | 0.855 | 0.896 | 0.024 | 2.93 |
| rust | narrow_tall_parquet_lazyframe_header | parquet_lazyframe | 40000 | 12 | 8192 | True (header) | 3 | 0.897 | 0.968 | 0.784 | 1.223 | 0.228 | 2.93 |
| rust | wide_medium_parquet_lazyframe_body | parquet_lazyframe | 10000 | 37 | 8192 | True (body) | 3 | 0.794 | 0.816 | 0.757 | 0.896 | 0.072 | 2.40 |
| rust | wide_medium_parquet_lazyframe_all | parquet_lazyframe | 10000 | 37 | 8192 | True (all) | 3 | 0.867 | 0.950 | 0.849 | 1.134 | 0.159 | 2.40 |

## Raw Timings (seconds)

- `rust / narrow_tall_dataframe_header`: `[0.8510761219076812, 0.8544315220788121, 0.9753779447637498]`
- `rust / wide_medium_dataframe_all`: `[1.0833025593310595, 1.130266711115837, 0.8095691339112818]`
- `rust / narrow_tall_parquet_lazyframe_none`: `[0.8551325178705156, 0.8964776787906885, 0.8959709377959371]`
- `rust / narrow_tall_parquet_lazyframe_header`: `[0.7841768842190504, 1.2231765128672123, 0.8973723719827831]`
- `rust / wide_medium_parquet_lazyframe_body`: `[0.7941322238184512, 0.7568247518502176, 0.8963703978806734]`
- `rust / wide_medium_parquet_lazyframe_all`: `[0.8488448350690305, 1.1335650281980634, 0.8673519450239837]`
