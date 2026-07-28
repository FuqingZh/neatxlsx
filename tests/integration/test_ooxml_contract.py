from __future__ import annotations

import json
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

import neatxlsx as nx
import polars as pl

NS = {"m": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}


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
