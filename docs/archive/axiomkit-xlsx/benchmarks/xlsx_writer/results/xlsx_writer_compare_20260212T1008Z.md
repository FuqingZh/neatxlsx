# XLSX Rust Benchmark Comparison (Post Fixes)

Comparison target:

- Previous rust runs: `xlsx_writer_20260212T085030Z.json`, `xlsx_writer_20260212T084801Z.json`
- Current rust runs: `xlsx_writer_20260212T100808Z.json`, `xlsx_writer_20260212T100817Z.json`

| scenario | old_rust_median_s | new_rust_median_s | new_vs_old |
| --- | ---: | ---: | ---: |
| huge_tall_header_autofit | 26.342 | 4.075 | -84.5% |
| huge_wide_autofit_all | 21.214 | 3.647 | -82.8% |
| narrow_tall_default | 3.571 | 0.593 | -83.4% |
| wide_medium_autofit_all | 2.613 | 0.565 | -78.4% |

Interpretation: negative means faster after fixes.
