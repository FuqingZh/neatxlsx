from __future__ import annotations

import hashlib
import json
import sys
from datetime import date, datetime, time, timedelta
from pathlib import Path

import neatxlsx as nx
import polars as pl


OUT = Path(__file__).resolve().parent
DEFAULT = OUT / "display-autofit-acceptance.xlsx"
NONZIP64 = OUT / "display-autofit-acceptance-nonzip64.xlsx"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_workbook(path: Path, *, use_zip64: bool) -> None:
    with nx.Workbook(
        path,
        keep_missing_values=True,
        missing_value="(missing)",
        chunk_size=2,
        use_zip64=use_zip64,
    ) as workbook:
        data = pl.DataFrame(
            {
                "latin_w": ["WWWWWWWWWW", "WWW Wide W"],
                "latin_i": ["iiiiiiiiii", "thin i"],
                "chinese": ["中文字段宽度验证", "中文，英文 Mixed"],
                "japanese": ["日本語の列幅検証", "全角カタカナ"],
                "korean": ["한국어 열 너비 검증", "한글과 English"],
                "fullwidth": ["ＡＢＣ１２３，。！？", "（）［］｛｝"],
                "emoji": ["emoji 😀🧬📈", "family 👨‍👩‍👧‍👦"],
                "decimal": [1234.6, -0.125],
                "grouping": [1234567.8, -9876.5],
                "percent": [0.1234, -0.5],
                "currency": [1234.5, -12.3],
                "fraction": [0.625, 2.75],
                "scientific": [1.2345e20, -2.5e-8],
                "date": [date(2026, 9, 1), date(1999, 12, 31)],
                "datetime": [datetime(2026, 9, 1, 14, 30, 15, 123000), datetime(1999, 12, 31, 23, 59, 59, 999000)],
                "time": [time(14, 30, 15, 123000), time(23, 59, 59, 999000)],
                "duration": [timedelta(hours=86, minutes=30, seconds=15, milliseconds=123), timedelta(seconds=1)],
                "missing": [None, "present"],
            }
        )
        groups = [
            "Typography", "Typography", "CJK", "CJK", "CJK", "CJK", "CJK",
            "Number formats", "Number formats", "Number formats", "Number formats",
            "Number formats", "Number formats", "Temporal", "Temporal", "Temporal",
            "Temporal", "Missing",
        ]
        header = pl.DataFrame({name: [group, name] for name, group in zip(data.columns, groups, strict=True)})
        formats = {
            "decimal": nx.Format(num_format="0.00"),
            "grouping": nx.Format(num_format="#,##0.00"),
            "percent": nx.Format(num_format="0.00%"),
            "currency": nx.Format(num_format='$#,##0.00;[Red]-$#,##0.00'),
            "fraction": nx.Format(num_format="# ?/?"),
            "scientific": nx.Format(num_format="0.00E+00"),
            "date": nx.Format(num_format="yyyy-mm-dd"),
            "datetime": nx.Format(num_format="yyyy-mm-dd hh:mm:ss.000"),
            "time": nx.Format(num_format="hh:mm:ss.000"),
            "duration": nx.Format(num_format="[h]:mm:ss.000"),
        }
        workbook.write_sheet(data, "Display matrix", header=header, merge_header=True,
                             column_formats=formats,
                             autofit=nx.Autofit(mode="all", max_rows=None, min_width=8, max_width=60, padding=2))
        workbook.write_sheet(
            pl.DataFrame({"minimum": ["i", "ii"], "maximum": ["W" * 160, "tail"], "padding": ["pad", "padding"]}),
            "Bounds", autofit=nx.Autofit(mode="all", min_width=12, max_width=18, padding=4),
        )
        workbook.write_sheet(
            pl.DataFrame({"sampled": ["a", "included-width", "EXCLUDED-VALUE-IS-INTENTIONALLY-MUCH-LONGER"], "missing": ["x", None, "tail"]}),
            "Sampling", autofit=nx.Autofit(mode="all", max_rows=2, min_width=8, max_width=60, padding=2),
        )
        workbook.write_sheet(
            pl.DataFrame({"bestfit_numeric": [1234.56], "edit_instruction": ["In desktop Excel, replace A2 with 123456789012345.67 and inspect the column."]}),
            "BestFit edit", column_formats={"bestfit_numeric": nx.Format(num_format="0.00")},
            autofit=nx.Autofit(mode="all", min_width=8, max_width=60, padding=2),
        )


write_workbook(DEFAULT, use_zip64=True)
write_workbook(NONZIP64, use_zip64=False)
(OUT / "generation.json").write_text(json.dumps({
    "source_id": "ab843776df868033",
    "candidate_python": sys.executable,
    "neatxlsx_module": nx.__file__,
    "default_zip64": {"path": DEFAULT.name, "sha256": sha256(DEFAULT)},
    "nonzip64": {"path": NONZIP64.name, "sha256": sha256(NONZIP64)},
    "covered": ["Latin W/i", "Chinese", "Japanese 日本語の列幅検証", "Korean", "fullwidth", "emoji", "number/time", "missing", "bounds", "sampling", "merged header", "bestFit"],
    "not_covered": ["real Excel-limit pagination", "Windows Excel bestFit edit"],
}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
