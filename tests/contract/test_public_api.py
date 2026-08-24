from __future__ import annotations

import inspect

import neatxlsx as nx


def test_public_surface_is_small_and_explicit() -> None:
    assert set(nx.__all__) == {
        "Autofit",
        "CommitError",
        "Error",
        "Format",
        "ScientificNotation",
        "SheetReport",
        "StateError",
        "Workbook",
        "WorksheetPart",
        "WriteError",
    }
    assert not hasattr(nx, "XlsxWriter")
    assert not hasattr(nx, "write_xlsx")


def test_write_sheet_uses_caller_facing_parameter_names() -> None:
    parameters = inspect.signature(nx.Workbook.write_sheet).parameters

    assert list(parameters) == [
        "self",
        "data",
        "sheet_name",
        "header",
        "header_row_formats",
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
    ]
    assert parameters["header_row_formats"].default is None
    assert parameters["column_formats"].default is None
    assert all(
        parameter.kind is inspect.Parameter.KEYWORD_ONLY
        for parameter in tuple(parameters.values())[3:]
    )


def test_additive_format_parameters_preserve_the_old_signature_order() -> None:
    parameters = inspect.signature(nx.Workbook.write_sheet).parameters

    assert [
        name
        for name in parameters
        if name not in {"header_row_formats", "column_formats"}
    ] == [
        "self",
        "data",
        "sheet_name",
        "header",
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
    ]


def test_exception_hierarchy_and_commit_metadata() -> None:
    error = nx.CommitError("report.xlsx", "denied")

    assert isinstance(error, nx.Error)
    assert error.target == "report.xlsx"
    assert error.retryable is True
