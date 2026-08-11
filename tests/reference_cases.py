"""Deterministic source data for the M0 report-reference artifacts."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC, date, datetime, time, timedelta
from decimal import Decimal
from typing import Any

import neatxlsx as nx
import polars as pl

EXCEL_MAX_ROWS = 1_048_576
EXCEL_MAX_COLUMNS = 16_384
MANIFEST_SCHEMA_VERSION = 1


@dataclass(frozen=True, slots=True)
class SheetSpec:
    """One logical sheet in a deterministic reference workbook."""

    name: str
    data: pl.DataFrame | pl.LazyFrame
    kwargs: dict[str, Any]
    sentinels: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class ReferenceCase:
    """Source inputs and expected-gap metadata for one reference scenario."""

    scenario_id: str
    sheets: tuple[SheetSpec, ...] = ()
    known_gaps: tuple[dict[str, str], ...] = ()
    split_planning: dict[str, Any] | None = None


def scenario_ids(*, include_large: bool = False) -> tuple[str, ...]:
    """Return reference scenarios in their stable generation order."""
    ids = (
        "ordinary-scientific",
        "mixed-dtype",
        "multi-level-header",
        "multi-sheet-pipeline",
        "split-planning",
    )
    if include_large:
        return (*ids, "large-row-split")
    return ids


def build_reference_case(scenario_id: str) -> ReferenceCase:
    """Build one deterministic reference case by its stable identifier."""
    builders = {
        "ordinary-scientific": _ordinary_scientific,
        "mixed-dtype": _mixed_dtype,
        "multi-level-header": _multi_level_header,
        "multi-sheet-pipeline": _multi_sheet_pipeline,
        "split-planning": _split_planning,
        "large-row-split": _large_row_split,
    }
    try:
        builder = builders[scenario_id]
    except KeyError as exc:
        raise ValueError(f"Unknown report-reference scenario: {scenario_id}") from exc
    return builder()


def _ordinary_scientific() -> ReferenceCase:
    data = pl.DataFrame(
        {
            "sample_id": ["S-001", "S-002", "S-003"],
            "count": [10, 2_000, 300_000],
            "effect": [0.0000123, 12.3456, 1_234_567.89],
            "p_value": [1.2e-8, 0.034, 1.2e12],
            "literal": ["=SUM(A2:A3)", "+cmd", "plain text"],
            "note": ["ok", None, "review"],
        }
    )
    return ReferenceCase(
        scenario_id="ordinary-scientific",
        sheets=(
            SheetSpec(
                name="Results",
                data=data,
                kwargs={
                    "integer_columns": "count",
                    "decimal_columns": ("effect", "p_value"),
                    "freeze_columns": 1,
                    "autofit": nx.Autofit(mode="all"),
                    "scientific_notation": nx.ScientificNotation(
                        scope="decimal",
                        min_absolute=1e-4,
                        max_absolute=1e6,
                    ),
                },
                sentinels=("A1", "B2", "C2", "D2", "E2", "E3", "F3"),
            ),
        ),
    )


def _mixed_dtype() -> ReferenceCase:
    data = pl.DataFrame(
        [
            pl.Series(
                "date",
                [date(2024, 1, 2), date(2024, 1, 3)],
                dtype=pl.Date,
            ),
            pl.Series(
                "datetime",
                [
                    datetime(2024, 1, 2, 3, 4, 5, 123456),
                    datetime(2024, 1, 3, 4, 5, 6, 123000),
                ],
                dtype=pl.Datetime("us"),
            ),
            pl.Series(
                "time",
                [time(3, 4, 5, 123456), time(4, 5, 6, 123000)],
                dtype=pl.Time,
            ),
            pl.Series(
                "duration",
                [
                    timedelta(seconds=1, microseconds=234),
                    timedelta(days=-1, microseconds=500),
                ],
                dtype=pl.Duration("us"),
            ),
            pl.Series(
                "decimal",
                [Decimal("12.30"), Decimal("999999999999999999.99")],
                dtype=pl.Decimal(20, 2),
            ),
            pl.Series("flag", [True, False], dtype=pl.Boolean),
            pl.Series("category", ["alpha", "beta"], dtype=pl.Categorical),
            pl.Series("enum", ["alpha", "beta"], dtype=pl.Enum(["alpha", "beta"])),
            pl.Series("literal", ["=1+1", "plain"], dtype=pl.String),
            pl.Series(
                "aware_datetime",
                [
                    datetime(2024, 1, 2, 3, 4, 5, tzinfo=UTC),
                    datetime(2024, 1, 3, 4, 5, 6, tzinfo=UTC),
                ],
                dtype=pl.Datetime("us", "UTC"),
            ),
        ]
    )
    return ReferenceCase(
        scenario_id="mixed-dtype",
        sheets=(
            SheetSpec(
                name="Mixed",
                data=data,
                kwargs={"autofit": nx.Autofit(mode="header")},
                sentinels=("A2", "F2", "I2", "J2"),
            ),
        ),
        known_gaps=(),
    )


def _multi_level_header() -> ReferenceCase:
    data = pl.DataFrame(
        {
            "id": [101, 102],
            "value": [1.25, 2.5],
            "note": ["first", "second"],
        }
    )
    header = pl.DataFrame(
        {
            "id": ["Results", "ID"],
            "value": ["Results", "Value"],
            "note": ["Metadata", "Note"],
        }
    )
    return ReferenceCase(
        scenario_id="multi-level-header",
        sheets=(
            SheetSpec(
                name="Header Report",
                data=data,
                kwargs={
                    "header": header,
                    "merge_header": True,
                    "freeze_columns": 1,
                },
                sentinels=("A1", "B1", "A2", "B2", "C3"),
            ),
        ),
    )


def _multi_sheet_pipeline() -> ReferenceCase:
    overview = pl.DataFrame({"metric": ["rows", "complete"], "value": [3, 1]})
    results = pl.DataFrame(
        {
            "sample": ["S-001", "S-002", "S-003"],
            "score": [0.25, 1.5, 12.75],
        }
    ).lazy()
    return ReferenceCase(
        scenario_id="multi-sheet-pipeline",
        sheets=(
            SheetSpec(
                name="Overview",
                data=overview,
                kwargs={"integer_columns": "value", "freeze_columns": 1},
                sentinels=("A1", "B2"),
            ),
            SheetSpec(
                name="Results",
                data=results,
                kwargs={
                    "decimal_columns": "score",
                    "autofit": nx.Autofit(mode="body"),
                },
                sentinels=("A1", "B2"),
            ),
        ),
    )


def _split_planning() -> ReferenceCase:
    header_rows = 2
    height = EXCEL_MAX_ROWS - header_rows + 1
    width = EXCEL_MAX_COLUMNS + 1
    max_data_rows = EXCEL_MAX_ROWS - header_rows
    row_slices = ((0, max_data_rows), (max_data_rows, height))
    col_slices = ((0, EXCEL_MAX_COLUMNS), (EXCEL_MAX_COLUMNS, width))
    parts = tuple(
        {
            "name": f"Split_{part_index}",
            "rows": [row_start, row_stop],
            "columns": [col_start, col_stop],
        }
        for part_index, (col_slice, row_slice) in enumerate(
            (
                (col_slice, row_slice)
                for col_slice in col_slices
                for row_slice in row_slices
            ),
            start=1,
        )
        for col_start, col_stop in (col_slice,)
        for row_start, row_stop in (row_slice,)
    )
    return ReferenceCase(
        scenario_id="split-planning",
        split_planning={
            "height": height,
            "width": width,
            "header_rows": header_rows,
            "parts": list(parts),
            "warning": "Excel limit overflow: split into 4 sheets (columns-first, then rows).",
        },
    )


def _large_row_split() -> ReferenceCase:
    rows = pl.int_range(0, EXCEL_MAX_ROWS, eager=False).alias("row")
    return ReferenceCase(
        scenario_id="large-row-split",
        sheets=(
            SheetSpec(
                name="Large Rows",
                data=pl.select(rows).lazy(),
                kwargs={"autofit": nx.Autofit(mode="none")},
                sentinels=(),
            ),
        ),
    )
