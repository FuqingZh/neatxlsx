"""Consistent XLSX report-table writing for Polars."""

from .errors import CommitError, Error, StateError, WriteError
from .spec import Autofit, Format, ScientificNotation, SheetReport, WorksheetPart
from .writer import Workbook

__all__ = [
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
]
