# XLSX Rust Benchmark Comparison

Comparison target:

- New rust-only runs: `xlsx_writer_20260212T085030Z.json`, `xlsx_writer_20260212T084801Z.json`

- Historical both-backend runs: `xlsx_writer_20260212T074644Z.json`, `xlsx_writer_20260212T074839Z.json`


| scenario | old_python_median_s | old_rust_median_s | new_rust_median_s | new_vs_old_python | new_vs_old_rust |
| --- | ---: | ---: | ---: | ---: | ---: |
| huge_tall_header_autofit | 20.014 | 26.836 | 26.342 | +31.6% | -1.8% |
| huge_wide_autofit_all | 16.076 | 21.522 | 21.214 | +32.0% | -1.4% |
| narrow_tall_default | 2.763 | 3.466 | 3.571 | +29.3% | +3.0% |
| wide_medium_autofit_all | 2.336 | 2.619 | 2.613 | +11.9% | -0.2% |

Interpretation: positive means slower (higher latency).
