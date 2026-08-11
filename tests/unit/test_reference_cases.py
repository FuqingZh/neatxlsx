from __future__ import annotations

import polars as pl
import pytest
from reference_cases import (
    MANIFEST_SCHEMA_VERSION,
    build_reference_case,
    scenario_ids,
)


def _schema(data: pl.DataFrame | pl.LazyFrame) -> pl.Schema:
    return data.schema if isinstance(data, pl.DataFrame) else data.collect_schema()


def test_reference_scenario_ids_are_unique_and_stable() -> None:
    ids = scenario_ids()

    assert ids == (
        "ordinary-scientific",
        "mixed-dtype",
        "multi-level-header",
        "multi-sheet-pipeline",
        "split-planning",
    )
    assert len(ids) == len(set(ids))
    assert scenario_ids(include_large=True)[-1] == "large-row-split"


@pytest.mark.parametrize("scenario_id", scenario_ids())
def test_reference_sources_have_deterministic_schema(scenario_id: str) -> None:
    first = build_reference_case(scenario_id)
    second = build_reference_case(scenario_id)

    assert first.scenario_id == second.scenario_id == scenario_id
    assert first.known_gaps == second.known_gaps
    assert first.split_planning == second.split_planning
    assert [(sheet.name, _schema(sheet.data)) for sheet in first.sheets] == [
        (sheet.name, _schema(sheet.data)) for sheet in second.sheets
    ]


def test_reference_manifest_schema_is_versioned() -> None:
    assert MANIFEST_SCHEMA_VERSION == 1


def test_unknown_reference_scenario_fails_clearly() -> None:
    with pytest.raises(ValueError, match="Unknown report-reference scenario"):
        build_reference_case("unknown")
