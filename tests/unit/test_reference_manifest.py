from __future__ import annotations

import copy

import pytest

from tests.reference_manifest import v5_compatibility_projection


def _manifest() -> dict[str, object]:
    return {
        "kind": "workbook",
        "reports": [{"warnings": []}],
        "sentinels": {"Data": [{"address": "A1", "value": "sentinel"}]},
        "styles": {"cell_xfs": [{"numFmtId": "0"}]},
        "worksheets": [
            {
                "name": "Data",
                "widths": {"A": 10.0, "B": 11.0},
                "merges": ["A1:B1"],
            }
        ],
        "ooxml": {
            "styles": {"cell_xfs": [{"numFmtId": "0"}]},
            "worksheets": [
                {
                    "name": "Data",
                    "columns": [
                        {
                            "min": "1",
                            "max": "2",
                            "width": "10",
                            "bestFit": "1",
                            "customWidth": "1",
                            "style": "3",
                            "hidden": "1",
                        }
                    ],
                }
            ],
        },
    }


def test_v5_projection_ignores_only_display_width_and_bestfit() -> None:
    baseline = _manifest()
    changed_display = copy.deepcopy(baseline)
    changed_display["worksheets"][0]["widths"] = {"A": 200.0}  # type: ignore[index]
    column = changed_display["ooxml"]["worksheets"][0]["columns"][0]  # type: ignore[index]
    column["width"] = "200"
    column.pop("bestFit")

    assert v5_compatibility_projection(changed_display) == v5_compatibility_projection(
        baseline
    )


@pytest.mark.parametrize(
    ("path", "value"),
    [
        (("ooxml", "worksheets", 0, "columns", 0, "min"), "2"),
        (("ooxml", "worksheets", 0, "columns", 0, "max"), "3"),
        (("ooxml", "worksheets", 0, "columns", 0, "style"), "4"),
        (("ooxml", "worksheets", 0, "columns", 0, "hidden"), "0"),
        (("ooxml", "worksheets", 0, "columns", 0, "customWidth"), "0"),
        (("ooxml", "styles", "cell_xfs", 0, "numFmtId"), "1"),
        (("sentinels", "Data", 0, "value"), "changed"),
        (("reports", 0, "warnings"), ["changed"]),
    ],
)
def test_v5_projection_rejects_structural_and_semantic_changes(
    path: tuple[object, ...], value: object
) -> None:
    baseline = _manifest()
    changed = copy.deepcopy(baseline)
    target: object = changed
    for key in path[:-1]:
        target = target[key]  # type: ignore[index]
    target[path[-1]] = value  # type: ignore[index]

    assert v5_compatibility_projection(changed) != v5_compatibility_projection(baseline)


def test_v5_projection_canonicalizes_grouped_columns_without_erasing_indexes() -> None:
    grouped = _manifest()
    split = copy.deepcopy(grouped)
    split["ooxml"]["worksheets"][0]["columns"] = [  # type: ignore[index]
        {
            "min": "1",
            "max": "1",
            "width": "20",
            "customWidth": "1",
            "style": "3",
            "hidden": "1",
        },
        {
            "min": "2",
            "max": "2",
            "width": "30",
            "bestFit": "1",
            "customWidth": "1",
            "style": "3",
            "hidden": "1",
        },
    ]
    assert v5_compatibility_projection(grouped) == v5_compatibility_projection(split)

    moved = copy.deepcopy(split)
    moved["ooxml"]["worksheets"][0]["columns"][1]["min"] = "3"  # type: ignore[index]
    assert v5_compatibility_projection(grouped) != v5_compatibility_projection(moved)
