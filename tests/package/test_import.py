from __future__ import annotations

import inspect
from importlib.metadata import version

import neatxlsx as nx
from neatxlsx import _native


def test_installed_distribution_and_extension_are_importable() -> None:
    parameters = inspect.signature(nx.Workbook.write_sheet).parameters

    assert version("neatxlsx") == "0.2.1"
    assert nx.Workbook.__module__ == "neatxlsx.writer"
    assert tuple(parameters) == (
        "self",
        "data",
        "sheet_name",
        "header",
        "header_row_formats",
        "header_column_formats",
        "column_formats",
        "integer_columns",
        "decimal_columns",
        "freeze_columns",
        "freeze_rows",
        "merge_header",
        "keep_missing_values",
        "autofit",
        "scientific_notation",
        "infer_numeric_columns",
        "infer_integer_columns",
    )
    assert parameters["header_row_formats"].default is None
    assert parameters["header_column_formats"].default is None
    assert parameters["column_formats"].default is None
    assert all(
        parameter.kind is inspect.Parameter.KEYWORD_ONLY
        for parameter in tuple(parameters.values())[3:]
    )
    assert _native.__build_profile__
    assert _native.__bridge_abi__ == 6
    assert _native.__bridge_contract__ == "neatxlsx.xlsx.writer.v6"
    assert _native.__bridge_transport__ == "arrow_c_data"
    assert hasattr(_native.XlsxWriter, "write_sheet_batches")
    assert not hasattr(_native.XlsxWriter, "write_sheet")
    assert not hasattr(_native.XlsxWriter, "write_sheet_batches_single_pass")
