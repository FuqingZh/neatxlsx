# LibreOffice canary: CJK-final candidate

Date: 2026-09-01

## Verdict

**LibreOffice-specific CJK visual canary passes for the tested Chinese,
Japanese, Korean, and fullwidth strings.** The final candidate's CJK widths
leave visible space before the next physical column in the Calc PDF. Latin,
numeric, and temporal samples are also readable.

This does not establish Windows Excel behavior. Windows Excel/COM is still
unavailable on this host, so the `bestFit` edit-expansion check remains unmet.
Emoji is retained as a font/rendering boundary rather than a width pass.

## Provenance

| Item | Value |
| --- | --- |
| Source id | `neatxlsx-cjk-1ca104048a7c-ab843776df868033` |
| Base head | `1ca104048a7c5663783d690fde54b1c75fc9a0ff` |
| Source snapshot SHA-256 | `ab843776df868033b868210e907f8bdfb1f5a130e4d2e5f1d8ffbad43c677a6c` |
| Candidate native SHA-256 | `449bbe218c8c33f092a51bdf6db04914751327b2b58f98ee7136898f989f72e1` |
| Candidate Python | `/tmp/neatxlsx-display-autofit-eval/candidate-cjk-ab843776df868033/.venv/bin/python` (Python 3.13.11) |
| Candidate dependencies | `rust_xlsxwriter 0.90.2`, `ssfmt 0.1.2` |
| LibreOffice | 6.4.7.2, Calc PDF and Calc Office Open XML filters |
| Reader | `openpyxl 3.1.5` |
| Platform | Linux 4.18.0-348.7.1.el8_5 x86_64 |
| Isolated LO profile | `/tmp/neatxlsx-display-autofit-eval/lo-profile-cjk-ab843776df868033` |

## Artifacts

| Artifact | SHA-256 | Result |
| --- | --- | --- |
| `display-autofit-acceptance.xlsx` | `b2e70bba990c70defb35e1a2e7239be5a6bd687ec59300c7c685922be0ce745c` | Default ZIP64 output; Calc 6.4 reported `source file could not be loaded`. |
| `display-autofit-acceptance-nonzip64.xlsx` | `5db45a7f60371663c29ca4651c0016d67a7d63c9df42711e6a781b289af55488` | Same source scenario with `use_zip64=False`; Calc opened it. |
| `pdf-nonzip64/display-autofit-acceptance-nonzip64.pdf` | `802c11a00ae874f613556d90225449f691600917edb88c9d34afedb09b80d3bf` | Calc-generated seven-page A4 PDF. |
| `roundtrip-nonzip64/display-autofit-acceptance-nonzip64.xlsx` | `0984ce747b9829596e885497f42187759448db372785a7b7c5cbcdee84bab5e4` | Calc Office Open XML round-trip. |

The default ZIP64 failure is recorded only as a LibreOffice 6.4 reader
compatibility observation. It does not modify the normal ZIP64 default.

`generation.json` records the exact small-workbook coverage: Latin `W/i`,
Chinese, `日本語の列幅検証`, Korean, fullwidth, emoji, decimal/grouping/
percent/currency/fraction/scientific, date/datetime/time/duration, missing
token, bounds/padding, sampling, merged header, and `bestFit`. It explicitly
does not claim real Excel-limit pagination coverage.

## PDF bbox and 150-DPI observations

The PDF bbox extraction is retained as
`pdf-nonzip64/display-autofit-acceptance-nonzip64.bbox.html`; 150-DPI pages are
under `pdf-pages-nonzip64/`.

- PASS: `中文字段宽度验证` is present as one bbox word from x=244.998 to
  x=332.982; the following Japanese cell begins at x=342.709.
- PASS: `日本語の列幅検証` is present in full from x=342.709 to x=430.693;
  the Korean cell begins at x=435.487.
- PASS: the full Korean sequence `한국어 열 너비 검증` is present as adjacent
  bbox words ending at x=523.735, with no next data column on that page.
- PASS: `ＡＢＣ１２３，。！？` is present in full from x=51.392 to x=161.372;
  the following emoji column begins at x=163.304.
- PASS: `WWWWWWWWWW`, `iiiiiiiiii`, decimal/grouping/percent/currency/fraction/
  scientific, and temporal strings including `86:30:15.123` are readable.
- EXPECTED: `Bounds.maximum` clips a deliberately 160-character W string at
  `max_width=18`; `Sampling` excludes its third long string because
  `max_rows=2`.
- UNVERIFIED: emoji glyphs are available to PDF text extraction but remain
  dependent on the host font fallback and Calc rendering. This is not evidence
  of cross-platform emoji-width accuracy.

## Round-trip structural observations

Full projections are in `roundtrip-analysis.json`.

- PASS: all four sheet names, all cell values, all cell data types, and all
  merged ranges remain equal after Calc open/save.
- CONCERN: Calc normalized six number-format strings in `Display matrix`
  (currency K3/K4; date N3/N4; datetime O3/O4). Values and types stayed equal.
- CONCERN: the source contains 19 `<col bestFit="1">` entries. Calc rounded
  stored widths to two decimals and removed every `bestFit` attribute on save.
  This is a Calc behavior; it is not an Excel `bestFit` edit result.

## Unmet boundaries

- No Windows mount, `powershell.exe`, `pwsh`, or `EXCEL.EXE` is available.
  Desktop Excel has not opened this artifact and `BestFit edit!A2` has not been
  changed to a longer numeric value.
- LibreOffice cannot substitute for the Excel edit-expansion check because it
  removes `bestFit` on round-trip.
- The compact canary does not exercise real Excel row/column-limit pagination.

All files are contained in this evaluation directory or its isolated temporary
LibreOffice profile. No repository, baseline, or candidate source was changed.
