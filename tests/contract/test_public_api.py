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
