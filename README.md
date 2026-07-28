# neatxlsx

`neatxlsx` is an opinionated XLSX report-table writer for Polars. It provides
consistent workbook formatting, multi-row headers, merge and freeze behavior,
and reliable streaming writes for large or wide tables.

```python
import neatxlsx as nx
import polars as pl

with nx.XlsxWriter("report.xlsx") as workbook:
    workbook.write_sheet(pl.DataFrame({"value": [1, 2]}), "Data")
```

