# XLSX Writer Benchmarks

This directory stores reproducible benchmark records for `axiomkit.io.XlsxWriter`.

## Run

From repo root:

```bash
pdm run python scripts/benchmark_xlsx_writer.py
```

Optional knobs:

```bash
pdm run python scripts/benchmark_xlsx_writer.py --repeat 3 --warmup 1
```

Huge-table profile:

```bash
pdm run python scripts/benchmark_xlsx_writer.py --profile huge --repeat 2 --warmup 1
```

## Output

Each run writes two files under `benchmarks/xlsx_writer/results/`:

- `xlsx_writer_<timestamp>.json`: machine-readable benchmark payload.
- `xlsx_writer_<timestamp>.md`: human-readable summary table.

## Scenarios

Current benchmark set includes:

- `default` profile:
  - `narrow_tall_default`: tall table with moderate width, default autofit policy.
  - `wide_medium_autofit_all`: medium-height, wide table with full-body autofit enabled.
- `huge` profile:
  - `huge_tall_header_autofit`: 250k rows, 15 columns.
  - `huge_wide_autofit_all`: 50k rows, 61 columns.

Backend:

- `rust`: `axiomkit.io.XlsxWriter` (single source of truth backend).
