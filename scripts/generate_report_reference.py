#!/usr/bin/env python3
"""Generate and verify deterministic M0 report-reference artifacts."""

from __future__ import annotations

import argparse
import difflib
import importlib.metadata
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

import neatxlsx as nx
from neatxlsx import _native

ROOT = Path(__file__).resolve().parents[1]
FIXTURE_DIR = ROOT / "tests" / "fixtures" / "report-reference"

if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tests.reference_cases import (  # noqa: E402
    ReferenceCase,
    build_reference_case,
    scenario_ids,
)
from tests.reference_manifest import (  # noqa: E402
    BASELINE_PROVENANCE,
    manifest_from_workbook,
)


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate and verify neatxlsx report-reference artifacts."
    )
    parser.add_argument(
        "--scenario",
        choices=scenario_ids(include_large=True),
        action="append",
        dest="scenarios",
        help="Generate one scenario; repeat to select several.",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("dist") / "report-reference",
    )
    parser.add_argument(
        "--include-large",
        action="store_true",
        help="Include the optional real row-limit XLSX artifact.",
    )
    parser.add_argument(
        "--accept",
        action="store_true",
        help="Replace expected JSON fixtures with the generated manifests.",
    )
    return parser.parse_args()


def _validate_baseline_acceptance() -> None:
    revision = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    actual = {
        "bridge_abi": _native.__bridge_abi__,
        "bridge_contract": _native.__bridge_contract__,
        "package_version": importlib.metadata.version("neatxlsx"),
        "source_revision": revision,
    }
    if actual != BASELINE_PROVENANCE:
        raise RuntimeError(
            "Baseline fixtures can only be accepted from the pinned v5 source: "
            f"expected {BASELINE_PROVENANCE!r}, got {actual!r}."
        )


def _write_case_workbook(
    output: Path, case: ReferenceCase
) -> tuple[nx.SheetReport, ...]:
    output.parent.mkdir(parents=True, exist_ok=True)
    with nx.Workbook(output, **(case.workbook_kwargs or {})) as workbook:
        for sheet in case.sheets:
            workbook.write_sheet(sheet.data, sheet.name, **sheet.kwargs)
        return workbook.report()


def generate_scenario(
    scenario_id: str,
    *,
    output_dir: Path,
    fixture_dir: Path = FIXTURE_DIR,
    accept: bool = False,
) -> dict[str, Any]:
    """Generate one scenario and compare or explicitly accept its manifest."""
    if accept:
        # Keep the immutable-oracle check at the mutation boundary.  Callers may
        # invoke this function directly instead of going through ``main()``.
        _validate_baseline_acceptance()
    case = build_reference_case(scenario_id)
    output_dir.mkdir(parents=True, exist_ok=True)
    output = output_dir / f"{scenario_id}.xlsx"
    reports = _write_case_workbook(output, case) if case.sheets else ()
    manifest = manifest_from_workbook(output, case, reports)
    actual_path = output_dir / f"{scenario_id}.json"
    actual_path.write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    expected_path = fixture_dir / f"{scenario_id}.json"
    if accept:
        expected_path.parent.mkdir(parents=True, exist_ok=True)
        expected_path.write_text(
            actual_path.read_text(encoding="utf-8"), encoding="utf-8"
        )
    elif not expected_path.exists():
        raise RuntimeError(
            f"Missing expected fixture {expected_path}; rerun with --accept."
        )
    else:
        expected = expected_path.read_text(encoding="utf-8").splitlines(keepends=True)
        actual = actual_path.read_text(encoding="utf-8").splitlines(keepends=True)
        if expected != actual:
            diff = "".join(
                difflib.unified_diff(
                    expected,
                    actual,
                    fromfile=str(expected_path),
                    tofile=str(actual_path),
                )
            )
            raise RuntimeError(f"Reference manifest mismatch:\n{diff}")
    artifacts = [str(actual_path)]
    if output.exists():
        artifacts.insert(0, str(output))
    print(f"{scenario_id}: {', '.join(artifacts)}")
    return manifest


def main() -> None:
    args = _parse_args()
    selected = tuple(args.scenarios or scenario_ids(include_large=args.include_large))
    if "large-row-split" in selected and not args.include_large:
        raise SystemExit("large-row-split requires --include-large")
    output_dir = args.output_dir
    for scenario_id in selected:
        generate_scenario(
            scenario_id,
            output_dir=output_dir,
            accept=args.accept,
        )


if __name__ == "__main__":
    main()
