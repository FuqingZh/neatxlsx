# neatxlsx

`neatxlsx` is an opinionated XLSX report-table writer for Polars. It turns
DataFrames and streaming LazyFrames into consistently formatted workbooks while
keeping multi-row headers, freeze panes, large/wide-table splitting, and
transactional delivery behind one small API.

## Installation

```bash
pip install neatxlsx
```

Python 3.11 or newer and Polars 1.38.1 or newer are required.

## Quick start

```python
import neatxlsx as nx
import polars as pl

data = pl.DataFrame(
    {
        "Sample": ["A", "B"],
        "Count": [12, 18],
        "Score": [0.125, 0.375],
    }
)

with nx.Workbook("report.xlsx") as workbook:
    workbook.write_sheet(
        data,
        "Results",
        integer_columns="Count",
        decimal_columns="Score",
        freeze_columns=1,
    )
```

`Workbook` writes to a sibling temporary file. Normal context-manager exit
atomically replaces the target where the filesystem supports atomic rename;
exceptional exit discards the temporary workbook and leaves an existing target
untouched.

## Formats and policies

```python
import neatxlsx as nx
import polars.selectors as cs

with nx.Workbook(
    "report.xlsx",
    header_format=nx.Format(bg_color="#D9EAF7"),
    decimal_format=nx.Format(num_format="0.000"),
) as workbook:
    workbook.write_sheet(
        data,
        "Results",
        integer_columns=cs.integer(),
        decimal_columns=cs.float(),
        autofit=nx.Autofit(mode="all", max_rows=10_000),
        scientific_notation=nx.ScientificNotation(scope="decimal"),
    )
```

Column arguments accept one name/index, an ordered sequence, or a Polars
selector. Explicit integer and decimal selections override inferred roles for
those columns; other columns remain inferred unless the matching `infer_*`
option is disabled.

## Large workbooks

ZIP64 is enabled by default because XLSX size depends on uncompressed worksheet
XML and cannot be predicted reliably before streaming. Use
`nx.Workbook(..., use_zip64=False)` only when a downstream reader lacks ZIP64
support. `Autofit(mode="header")` and `"none"` preserve a single LazyFrame
evaluation; `"body"` and `"all"` evaluate it twice.

See [the documentation map](docs/README.md) for the complete API, lifecycle,
testing, and release contracts.

