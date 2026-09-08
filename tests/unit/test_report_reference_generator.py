from __future__ import annotations

import importlib.util
import json
from pathlib import Path

import pytest

SCRIPT = Path(__file__).parents[2] / "scripts" / "generate_report_reference.py"
SPEC = importlib.util.spec_from_file_location("generate_report_reference", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
generate_report_reference = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(generate_report_reference)


def test_generator_does_not_replace_fixture_without_accept(tmp_path: Path) -> None:
    fixture_dir = tmp_path / "fixtures"
    fixture_dir.mkdir()
    source_fixture = (
        Path(__file__).parents[1]
        / "fixtures"
        / "report-reference"
        / "ordinary-scientific.json"
    )
    fixture = fixture_dir / "ordinary-scientific.json"
    fixture.write_bytes(source_fixture.read_bytes())
    before = fixture.read_bytes()

    generate_report_reference.generate_scenario(
        "ordinary-scientific",
        output_dir=tmp_path / "output",
        fixture_dir=fixture_dir,
    )

    assert fixture.read_bytes() == before
    actual = json.loads(
        (tmp_path / "output" / "ordinary-scientific.json").read_text(encoding="utf-8")
    )
    assert actual["worksheets"][0]["widths"]
    assert actual["ooxml"]["worksheets"][0]["columns"][0]["width"]


def test_direct_accept_validates_frozen_baseline_before_writing(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    def reject_candidate() -> None:
        raise RuntimeError("candidate is not the pinned baseline")

    monkeypatch.setattr(
        generate_report_reference, "_validate_baseline_acceptance", reject_candidate
    )

    fixture_dir = tmp_path / "fixtures"
    output_dir = tmp_path / "output"
    with pytest.raises(RuntimeError, match="not the pinned baseline"):
        generate_report_reference.generate_scenario(
            "ordinary-scientific",
            output_dir=output_dir,
            fixture_dir=fixture_dir,
            accept=True,
        )

    assert not fixture_dir.exists()
    assert not output_dir.exists()


def test_generator_reports_field_level_manifest_mismatch(tmp_path: Path) -> None:
    fixture_dir = tmp_path / "fixtures"
    fixture_dir.mkdir()
    (fixture_dir / "ordinary-scientific.json").write_text("{}\n", encoding="utf-8")

    with pytest.raises(RuntimeError, match="Reference manifest mismatch"):
        generate_report_reference.generate_scenario(
            "ordinary-scientific",
            output_dir=tmp_path / "output",
            fixture_dir=fixture_dir,
        )


def test_split_generator_emits_only_compact_json(tmp_path: Path) -> None:
    fixture_dir = tmp_path / "fixtures"
    fixture_dir.mkdir()
    source_fixture = (
        Path(__file__).parents[1]
        / "fixtures"
        / "report-reference"
        / "split-planning.json"
    )
    (fixture_dir / "split-planning.json").write_bytes(source_fixture.read_bytes())

    generate_report_reference.generate_scenario(
        "split-planning",
        output_dir=tmp_path / "output",
        fixture_dir=fixture_dir,
    )

    assert (tmp_path / "output" / "split-planning.json").exists()
    assert not (tmp_path / "output" / "split-planning.xlsx").exists()
