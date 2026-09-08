from __future__ import annotations

import json
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

import neatxlsx as nx
import polars as pl

NS = {"m": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}


def _normalize_zip_member_for_autofit_comparison(name: str, raw: bytes) -> bytes:
    if name == "docProps/core.xml":
        root = ET.fromstring(raw)
        for element in root:
            if element.tag.rsplit("}", maxsplit=1)[-1] in {"created", "modified"}:
                element.text = ""
        return ET.tostring(root, encoding="utf-8")
    if not name.startswith("xl/worksheets/"):
        return raw
    root = ET.fromstring(raw)
    columns = root.find("m:cols", NS)
    if columns is not None:
        root.remove(columns)
    return ET.tostring(root, encoding="utf-8")


def test_canonical_workbook_manifest_and_ooxml_structure(tmp_path: Path) -> None:
    output = tmp_path / "contract.xlsx"
    header = pl.DataFrame({"a": ["Group", "ID"], "b": ["Group", "Value"]})
    with nx.Workbook(output, use_zip64=False) as workbook:
        workbook.write_sheet(
            pl.DataFrame({"a": [1], "b": [2.5]}),
            "Data",
            header=header,
            merge_header=True,
        )
        reports = workbook.report()

    manifest = {
        "requested_name": reports[0].requested_name,
        "worksheets": [
            {
                "name": part.name,
                "rows": [part.row_start, part.row_stop],
                "columns": [part.column_start, part.column_stop],
            }
            for part in reports[0].worksheets
        ],
        "warnings": list(reports[0].warnings),
    }
    assert json.dumps(manifest, sort_keys=True) == json.dumps(
        {
            "requested_name": "Data",
            "worksheets": [{"name": "Data", "rows": [0, 1], "columns": [0, 2]}],
            "warnings": [],
        },
        sort_keys=True,
    )
    with zipfile.ZipFile(output) as archive:
        names = set(archive.namelist())
        assert {
            "[Content_Types].xml",
            "xl/workbook.xml",
            "xl/styles.xml",
            "xl/worksheets/sheet1.xml",
        } <= names
        root = ET.fromstring(archive.read("xl/worksheets/sheet1.xml"))
    merged = root.find("m:mergeCells/m:mergeCell", NS)
    assert merged is not None
    assert merged.attrib["ref"] == "A1:B1"


def test_autofit_writes_bestfit_columns_and_none_leaves_columns_unset(
    tmp_path: Path,
) -> None:
    autofit_output = tmp_path / "autofit.xlsx"
    none_output = tmp_path / "none.xlsx"

    with nx.Workbook(autofit_output, use_zip64=False) as workbook:
        workbook.write_sheet(
            pl.DataFrame({"amount": [1234.5]}),
            "Data",
            column_formats={"amount": nx.Format(num_format="#,##0.00")},
            autofit=nx.Autofit(mode="body", min_width=8, max_width=60, padding=2),
        )
    with nx.Workbook(none_output, use_zip64=False) as workbook:
        workbook.write_sheet(
            pl.DataFrame({"amount": [1234.5]}),
            "Data",
            column_formats={"amount": nx.Format(num_format="#,##0.00")},
            autofit=nx.Autofit(mode="none"),
        )

    with zipfile.ZipFile(autofit_output) as archive:
        autofit_root = ET.fromstring(archive.read("xl/worksheets/sheet1.xml"))
    with zipfile.ZipFile(none_output) as archive:
        none_root = ET.fromstring(archive.read("xl/worksheets/sheet1.xml"))

    column = autofit_root.find("m:cols/m:col", NS)
    assert column is not None
    assert column.attrib["min"] == "1"
    assert column.attrib["max"] == "1"
    assert column.attrib["bestFit"] == "1"
    assert column.attrib["customWidth"] == "1"
    assert float(column.attrib["width"]) >= 8
    assert none_root.find("m:cols", NS) is None

    with (
        zipfile.ZipFile(autofit_output) as autofit_archive,
        zipfile.ZipFile(none_output) as none_archive,
    ):
        assert set(autofit_archive.namelist()) == set(none_archive.namelist())
        for name in autofit_archive.namelist():
            assert _normalize_zip_member_for_autofit_comparison(
                name, autofit_archive.read(name)
            ) == _normalize_zip_member_for_autofit_comparison(
                name, none_archive.read(name)
            )


def test_measured_east_asian_fallback_widens_only_eligible_text(
    tmp_path: Path,
) -> None:
    widths: dict[str, float] = {}
    for label, value in {
        "east_asian": "中",
        "emoji": "😀",
        "accented_latin": "é",
    }.items():
        output = tmp_path / f"{label}.xlsx"
        with nx.Workbook(output, use_zip64=False) as workbook:
            workbook.write_sheet(
                pl.DataFrame({"value": [value]}),
                "Data",
                autofit=nx.Autofit(mode="body", min_width=1, padding=0),
            )
        with zipfile.ZipFile(output) as archive:
            root = ET.fromstring(archive.read("xl/worksheets/sheet1.xml"))
        column = root.find("m:cols/m:col", NS)
        assert column is not None
        widths[label] = float(column.attrib["width"])

    assert widths["east_asian"] > widths["accented_latin"]
    assert widths["emoji"] == widths["accented_latin"]
