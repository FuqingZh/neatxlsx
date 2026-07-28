from __future__ import annotations

import warnings
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

import polars as pl
import pytest

from neatxlsx import XlsxWriter  # noqa: E402
from neatxlsx._rs_bridge import is_rs_backend_available  # noqa: E402
from neatxlsx.spec import (  # noqa: E402
    AutofitPolicy,
    ScientificPolicy,
    XlsxRowChunkPolicy,
    XlsxValuePolicy,
    XlsxWriteOptions,
)

NS_MAIN = {"m": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
DICT_NUM_FMT_BUILTIN = {
    0: "General",
    1: "0",
    2: "0.00",
    3: "#,##0",
    4: "#,##0.00",
    9: "0%",
    10: "0.00%",
    11: "0.00E+00",
    12: "# ?/?",
    13: "# ??/??",
    14: "mm-dd-yy",
    49: "@",
}


def _read_shared_strings(zf: zipfile.ZipFile) -> list[str]:
    try:
        v_xml = zf.read("xl/sharedStrings.xml")
    except KeyError:
        return []

    root = ET.fromstring(v_xml)
    l_strings: list[str] = []
    for node_si in root.findall(".//m:si", NS_MAIN):
        l_text_nodes = node_si.findall(".//m:t", NS_MAIN)
        l_strings.append("".join((node.text or "") for node in l_text_nodes))
    return l_strings


def _read_styles(zf: zipfile.ZipFile) -> tuple[list[ET.Element], dict[int, str]]:
    v_xml = zf.read("xl/styles.xml")
    root = ET.fromstring(v_xml)

    dict_num_fmts: dict[int, str] = {}
    for node_fmt in root.findall(".//m:numFmts/m:numFmt", NS_MAIN):
        n_id = int(node_fmt.attrib["numFmtId"])
        dict_num_fmts[n_id] = node_fmt.attrib["formatCode"]

    node_cell_xfs = root.find(".//m:cellXfs", NS_MAIN)
    assert node_cell_xfs is not None
    l_xfs = list(node_cell_xfs.findall("m:xf", NS_MAIN))
    return l_xfs, dict_num_fmts


def _resolve_num_format_code(style_idx: int, l_xfs: list[ET.Element], dict_num_fmts: dict[int, str]) -> str:
    node_xf = l_xfs[style_idx]
    n_fmt_id = int(node_xf.attrib.get("numFmtId", "0"))
    return dict_num_fmts.get(n_fmt_id, DICT_NUM_FMT_BUILTIN.get(n_fmt_id, f"numFmtId:{n_fmt_id}"))


def read_cell(path_xlsx: Path, cell_ref: str) -> tuple[str | None, str, str]:
    with zipfile.ZipFile(path_xlsx) as zf:
        l_shared_strings = _read_shared_strings(zf)
        l_xfs, dict_num_fmts = _read_styles(zf)
        root_sheet = ET.fromstring(zf.read("xl/worksheets/sheet1.xml"))

    node_cell = root_sheet.find(f".//m:c[@r='{cell_ref}']", NS_MAIN)
    assert node_cell is not None, f"Missing cell: {cell_ref}"

    c_type = node_cell.attrib.get("t")
    n_style_idx = int(node_cell.attrib.get("s", "0"))
    c_fmt = _resolve_num_format_code(n_style_idx, l_xfs, dict_num_fmts)

    node_value = node_cell.find("m:v", NS_MAIN)
    c_raw = node_value.text if node_value is not None and node_value.text is not None else ""
    if c_type == "s":
        return c_type, l_shared_strings[int(c_raw)], c_fmt
    if c_type == "inlineStr":
        nodes_text = node_cell.findall(".//m:is/m:t", NS_MAIN)
        return "s", "".join((node.text or "") for node in nodes_text), c_fmt
    return c_type, c_raw, c_fmt


def test_integer_strict_vs_coerce_semantics(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    df = pl.DataFrame({"x": [1.0, 2.5]})

    path_file_strict = tmp_path / "strict.xlsx"
    with XlsxWriter(path_file_strict) as writer:
        writer.write_sheet(
            body=df,
            sheet_name="S",
            cols_integer=["x"],
            policy_autofit=AutofitPolicy(mode="none"),
        )

    c_type_a2, c_value_a2, _ = read_cell(path_file_strict, "A2")
    c_type_a3, c_value_a3, _ = read_cell(path_file_strict, "A3")
    assert c_type_a2 != "s"
    assert float(c_value_a2) == 1.0
    assert c_type_a3 == "s"
    assert c_value_a3 == "2.5"

    path_file_coerce = tmp_path / "coerce.xlsx"
    opts_coerce = XlsxWriteOptions(
        value_policy=XlsxValuePolicy(integer_coerce="coerce")
    )
    with XlsxWriter(path_file_coerce, options_write=opts_coerce) as writer:
        writer.write_sheet(
            body=df,
            sheet_name="S",
            cols_integer=["x"],
            policy_autofit=AutofitPolicy(mode="none"),
        )

    c_type_b3, c_value_b3, _ = read_cell(path_file_coerce, "A3")
    assert c_type_b3 != "s"
    assert float(c_value_b3) == 2.0


def test_infer_numeric_uses_decimal_format_by_default(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    df = pl.DataFrame({"metric": [1.2345, 2.5], "label": ["a", "b"]})
    path_file_out = tmp_path / "decimal_fmt.xlsx"

    with XlsxWriter(path_file_out) as writer:
        writer.write_sheet(
            body=df,
            sheet_name="S",
            policy_autofit=AutofitPolicy(mode="none"),
            policy_scientific=ScientificPolicy(scope="decimal"),
        )

    c_type, _, c_fmt = read_cell(path_file_out, "A2")
    assert c_type != "s"
    assert c_fmt == "0.0000"


def test_scientific_format_trigger_for_extreme_values(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    df = pl.DataFrame({"metric": [1e-8, 2e-8, 3e-8]})
    path_file_out = tmp_path / "scientific_fmt.xlsx"

    with XlsxWriter(path_file_out) as writer:
        writer.write_sheet(
            body=df,
            sheet_name="S",
            policy_autofit=AutofitPolicy(mode="none"),
            policy_scientific=ScientificPolicy(scope="decimal"),
        )

    c_type, _, c_fmt = read_cell(path_file_out, "A2")
    assert c_type != "s"
    assert "E+" in c_fmt


def test_numeric_string_selector_targets_named_column_and_warns(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    df = pl.DataFrame({"x": [1.0, 2.0], "0": [2.5, 3.5]})
    path_file_out = tmp_path / "numeric_name_selector.xlsx"

    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        with XlsxWriter(path_file_out) as writer:
            writer.write_sheet(
                body=df,
                sheet_name="S",
                cols_integer=["0"],
                policy_autofit=AutofitPolicy(mode="none"),
            )

    messages = [str(item.message) for item in caught]
    assert any("literal column names" in message for message in messages)

    c_type_a2, c_value_a2, _ = read_cell(path_file_out, "A2")
    c_type_b2, c_value_b2, _ = read_cell(path_file_out, "B2")
    assert c_type_a2 != "s"
    assert float(c_value_a2) == 1.0
    assert c_type_b2 == "s"
    assert c_value_b2 == "2.5"


def test_integer_index_selector_uses_zero_based_index_without_warning(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    df = pl.DataFrame({"x": [1.0, 2.0], "0": [2.5, 3.5]})
    path_file_out = tmp_path / "integer_index_selector.xlsx"

    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        with XlsxWriter(path_file_out) as writer:
            writer.write_sheet(
                body=df,
                sheet_name="S",
                cols_integer=[0],
                policy_autofit=AutofitPolicy(mode="none"),
            )

    messages = [str(item.message) for item in caught]
    assert not any("literal column names" in message for message in messages)

    c_type_a2, c_value_a2, _ = read_cell(path_file_out, "A2")
    c_type_b2, c_value_b2, _ = read_cell(path_file_out, "B2")
    assert c_type_a2 != "s"
    assert float(c_value_a2) == 1.0
    assert c_type_b2 != "s"
    assert float(c_value_b2) == 2.5


def test_scientific_policy_scope_none_disables_scientific(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    df = pl.DataFrame({"metric": [1e-8, 2e-8, 3e-8]})
    path_file_out = tmp_path / "scientific_scope_none.xlsx"

    with XlsxWriter(path_file_out) as writer:
        writer.write_sheet(
            body=df,
            sheet_name="S",
            policy_autofit=AutofitPolicy(mode="none"),
            policy_scientific=ScientificPolicy(scope="none"),
        )

    c_type, _, c_fmt = read_cell(path_file_out, "A2")
    assert c_type != "s"
    assert "E+" not in c_fmt


def test_scientific_policy_default_disabled(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    df = pl.DataFrame({"metric": [1e-8, 2e-8, 3e-8]})
    path_file_out = tmp_path / "scientific_default_disabled.xlsx"

    with XlsxWriter(path_file_out) as writer:
        writer.write_sheet(
            body=df,
            sheet_name="S",
            policy_autofit=AutofitPolicy(mode="none"),
        )

    c_type, _, c_fmt = read_cell(path_file_out, "A2")
    assert c_type != "s"
    assert "E+" not in c_fmt


def test_scientific_policy_scope_integer_applies_to_integer_cols(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    df = pl.DataFrame({"metric": [10_000_000, 20_000_000, 30_000_000]})
    path_file_out = tmp_path / "scientific_scope_integer.xlsx"

    with XlsxWriter(path_file_out) as writer:
        writer.write_sheet(
            body=df,
            sheet_name="S",
            policy_autofit=AutofitPolicy(mode="none"),
            policy_scientific=ScientificPolicy(
                scope="integer",
                thr_min=0.0001,
                thr_max=1_000_000.0,
            ),
        )

    c_type, _, c_fmt = read_cell(path_file_out, "A2")
    assert c_type != "s"
    assert "E+" in c_fmt


def test_row_chunk_policy_is_active_in_write_path(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    df = pl.DataFrame({"x": [1, 2, 3]})

    opts_bad = XlsxWriteOptions(
        row_chunk_policy=XlsxRowChunkPolicy(fixed_size=0)
    )
    with XlsxWriter(tmp_path / "bad_chunk.xlsx", options_write=opts_bad) as writer:
        with pytest.raises(ValueError, match="row_chunk_policy"):
            writer.write_sheet(
                body=df,
                sheet_name="S",
                policy_autofit=AutofitPolicy(mode="none"),
            )

    opts_ok = XlsxWriteOptions(
        row_chunk_policy=XlsxRowChunkPolicy(fixed_size=1)
    )
    path_file_ok = tmp_path / "ok_chunk.xlsx"
    with XlsxWriter(path_file_ok, options_write=opts_ok) as writer:
        writer.write_sheet(
            body=df,
            sheet_name="S",
            policy_autofit=AutofitPolicy(mode="none"),
        )

    _, c_value, _ = read_cell(path_file_ok, "A4")
    assert float(c_value) == 3.0
