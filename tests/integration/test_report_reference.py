from __future__ import annotations

import json
from pathlib import Path

import neatxlsx as nx
import pytest
from reference_cases import build_reference_case, scenario_ids
from reference_manifest import manifest_from_workbook

ROOT = Path(__file__).parents[2]
FIXTURE_DIR = ROOT / "tests" / "fixtures" / "report-reference"


@pytest.mark.parametrize(
    "scenario_id",
    tuple(
        scenario_id for scenario_id in scenario_ids() if scenario_id != "split-planning"
    ),
)
def test_reference_workbook_matches_manifest(tmp_path: Path, scenario_id: str) -> None:
    case = build_reference_case(scenario_id)
    expected = json.loads(
        (FIXTURE_DIR / f"{scenario_id}.json").read_text(encoding="utf-8")
    )
    output = tmp_path / f"{scenario_id}.xlsx"
    with nx.Workbook(output, **(case.workbook_kwargs or {})) as workbook:
        for sheet in case.sheets:
            workbook.write_sheet(sheet.data, sheet.name, **sheet.kwargs)
        reports = workbook.report()

    assert manifest_from_workbook(output, case, reports) == expected


def test_split_planning_manifest_is_compact_and_columns_first() -> None:
    expected = json.loads(
        (FIXTURE_DIR / "split-planning.json").read_text(encoding="utf-8")
    )
    case = build_reference_case("split-planning")

    assert manifest_from_workbook(Path(), case, ()) == expected
    assert [part["name"] for part in expected["split_planning"]["parts"]] == [
        "Split_1",
        "Split_2",
        "Split_3",
        "Split_4",
    ]
