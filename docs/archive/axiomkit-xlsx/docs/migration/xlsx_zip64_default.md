# XLSX ZIP64 Default

`XlsxWriteOptions.should_use_zip64` now controls whether the XLSX container uses
ZIP64 extensions. It defaults to `True` so workbooks can exceed the standard ZIP
4 GiB limit without failing during `XlsxWriter.close()`.

Existing Python callers do not need to change:

```python
with XlsxWriter("report.xlsx") as writer:
    writer.write_sheet(df, "results")
```

Callers that require classic ZIP output for a legacy non-Excel reader can opt
out explicitly:

```python
options = XlsxWriteOptions(should_use_zip64=False)
with XlsxWriter("report.xlsx", options_write=options) as writer:
    writer.write_sheet(df, "results")
```

ZIP64 changes only the outer XLSX ZIP container. Worksheet contents, formulas,
styles, compression, and Excel row and column limits are unchanged.
