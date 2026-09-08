from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from dataclasses import asdict
from pathlib import Path
from typing import Any

import pytest

SCRIPTS = Path(__file__).parents[2] / "scripts"
sys.path.insert(0, str(SCRIPTS))
SCRIPT = SCRIPTS / "compare_xlsx_writer.py"
SPEC = importlib.util.spec_from_file_location("compare_xlsx_writer", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
comparison = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = comparison
SPEC.loader.exec_module(comparison)


def _analysis_item(verdict: str) -> dict[str, object]:
    return {
        "scenario": {"name": "scenario", "rule_autofit_columns": "body"},
        "samples_by_variant": {"baseline": 8, "candidate": 8},
        "metrics": {"total_wall_s": {"median_ratio": 1.0, "ci95": [1.0, 1.0]}},
        "zip_members_equal": True,
        "verdict": verdict,
    }


def _finalize(
    tmp_path: Path,
    *,
    verdicts: list[str],
    acceptance_eligible: bool = True,
    worker_failed: bool = False,
) -> tuple[int, dict[str, object]]:
    code = comparison.finalize_comparison(
        out_dir=tmp_path,
        planned_payload={
            "timestamp_utc": "2026-09-01T00:00:00+00:00",
            "acceptance_eligible": acceptance_eligible,
            "order_seed": 1,
            "analysis_seed": 2,
        },
        verdict_policy="display-autofit",
        analysis=[_analysis_item(verdict) for verdict in verdicts],
        records=[],
        worker_failed=worker_failed,
    )
    payload = json.loads((tmp_path / "comparison.json").read_text(encoding="utf-8"))
    return code, payload


def test_finalize_accepts_passed_required_verdicts_and_diagnostic_control(
    tmp_path: Path,
) -> None:
    code, payload = _finalize(tmp_path, verdicts=["passed", "diagnostic"])

    assert code == 0
    assert payload["status"] == "complete"
    assert payload["acceptance_passed"] is True
    assert "Acceptance passed: `true`" in (tmp_path / "comparison.md").read_text()


@pytest.mark.parametrize("verdict", ["failed", "inconclusive", "rejected"])
def test_finalize_rejects_nonpassing_required_verdicts(
    tmp_path: Path, verdict: str
) -> None:
    code, payload = _finalize(tmp_path, verdicts=["passed", verdict])

    assert code == 1
    assert payload["status"] == "complete"
    assert payload["acceptance_passed"] is False


def test_finalize_fails_when_a_worker_failed(tmp_path: Path) -> None:
    code, payload = _finalize(tmp_path, verdicts=["passed"], worker_failed=True)

    assert code == 1
    assert payload["status"] == "incomplete"
    assert payload["acceptance_passed"] is False


def test_finalize_override_is_nonacceptance_but_not_worker_failure(
    tmp_path: Path,
) -> None:
    code, payload = _finalize(
        tmp_path,
        verdicts=["diagnostic"],
        acceptance_eligible=False,
    )

    assert code == 0
    assert payload["status"] == "complete"
    assert payload["acceptance_passed"] is None
    assert (
        "Acceptance passed: `n/a (non-acceptance override)`"
        in (tmp_path / "comparison.md").read_text()
    )


@pytest.mark.parametrize(
    ("verdict", "expected_exit", "acceptance_passed"),
    [("passed", 0, True), ("failed", 1, False)],
)
def test_main_wires_final_payload_and_exit_code(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    verdict: str,
    expected_exit: int,
    acceptance_passed: bool,
) -> None:
    out_dir = tmp_path / "comparison"
    scenario = comparison.benchmark.XlsxBenchmarkScenario(
        name="scenario",
        n_rows=1,
        n_numeric_cols=1,
        n_text_cols=0,
        rule_autofit_columns="body",
        input_kind="parquet_lazyframe",
    )
    variant = comparison.VariantConfig(
        "baseline", tmp_path / "baseline", tmp_path / "python"
    )
    args = argparse.Namespace(
        blocks=1,
        polars_threads=1,
        bootstrap_resamples=1,
        timeout_seconds=1.0,
        cpu_list=None,
        baseline_root=variant.checkout_root,
        baseline_python=variant.python,
        candidate_root=tmp_path / "candidate",
        candidate_python=tmp_path / "candidate-python",
        out_dir=out_dir,
        profile="huge",
        include_full_scan=False,
        include_computed=False,
        scenarios=None,
        warmup=0,
        order_seed=1,
        analysis_seed=2,
        allow_dirty_source=False,
        allow_identical_native=False,
        baseline_source_id=None,
        candidate_source_id=None,
        verdict_policy="display-autofit",
        keep_workbooks=False,
    )
    monkeypatch.setattr(comparison, "parse_args", lambda: args)
    monkeypatch.setattr(
        comparison, "validate_comparison_paths", lambda *_: (variant, variant)
    )
    monkeypatch.setattr(
        comparison, "build_comparison_scenarios", lambda *_args, **_kwargs: [scenario]
    )
    monkeypatch.setattr(comparison, "create_fixtures", lambda *_: {})
    monkeypatch.setattr(comparison, "generate_run_plan", lambda *_args, **_kwargs: [])
    monkeypatch.setattr(comparison, "_root_metadata", lambda *_args, **_kwargs: {})
    monkeypatch.setattr(
        comparison,
        "analyze_scenario",
        lambda *_args, **_kwargs: _analysis_item(verdict),
    )

    assert comparison.main() == expected_exit
    payload = json.loads((out_dir / "comparison.json").read_text(encoding="utf-8"))
    assert payload["status"] == "complete"
    assert payload["acceptance_passed"] is acceptance_passed


def test_run_plan_is_deterministic_balanced_and_paired() -> None:
    first = comparison.generate_run_plan("scenario", blocks=4, warmup=True, seed=17)
    second = comparison.generate_run_plan("scenario", blocks=4, warmup=True, seed=17)

    assert first == second
    warmups = [run for run in first if run.warmup]
    measured = [run for run in first if not run.warmup]
    assert {run.variant for run in warmups} == {"baseline", "candidate"}
    assert sum(run.variant == "baseline" for run in measured) == 8
    assert sum(run.variant == "candidate" for run in measured) == 8
    for block in range(4):
        block_runs = [run for run in measured if run.block == block]
        assert [run.variant for run in block_runs] in [
            ["baseline", "candidate", "candidate", "baseline"],
            ["candidate", "baseline", "baseline", "candidate"],
        ]
        for pair in range(2):
            variants = {run.variant for run in block_runs if run.position // 2 == pair}
            assert variants == {"baseline", "candidate"}


def test_comparison_paths_require_distinct_roots_and_external_output(
    tmp_path: Path,
) -> None:
    variants = []
    for label in ("baseline", "candidate"):
        root = tmp_path / label
        (root / "python" / "neatxlsx").mkdir(parents=True)
        python = root / ".venv" / "bin" / "python"
        python.parent.mkdir(parents=True)
        python.touch()
        variants.append(comparison.VariantConfig(label, root, python))

    resolved = comparison.validate_comparison_paths(
        (variants[0], variants[1]), tmp_path / "results"
    )
    assert resolved[0].checkout_root == variants[0].checkout_root.resolve()
    assert resolved[0].python == variants[0].python.absolute()

    with pytest.raises(ValueError, match="must not overlap"):
        comparison.validate_comparison_paths(
            (variants[0], variants[1]), variants[0].checkout_root / "results"
        )


def test_comparison_preserves_virtualenv_python_symlink(tmp_path: Path) -> None:
    system_python = tmp_path / "system-python"
    system_python.touch()
    variants = []
    for label in ("baseline", "candidate"):
        root = tmp_path / label
        (root / "python" / "neatxlsx").mkdir(parents=True)
        python = root / ".venv" / "bin" / "python"
        python.parent.mkdir(parents=True)
        python.symlink_to(system_python)
        variants.append(comparison.VariantConfig(label, root, python))

    resolved = comparison.validate_comparison_paths(
        (variants[0], variants[1]), tmp_path / "results"
    )
    assert resolved[0].python == variants[0].python.absolute()
    assert resolved[0].python != system_python.resolve()


def test_bootstrap_median_interval_is_deterministic() -> None:
    first = comparison.bootstrap_median_ci(
        [0.90, 0.92, 0.94, 0.96], seed=31, resamples=500
    )
    second = comparison.bootstrap_median_ci(
        [0.90, 0.92, 0.94, 0.96], seed=31, resamples=500
    )
    assert first == second
    assert first[0] <= 0.93 <= first[1]


def test_full_scan_adds_lazy_body_and_all_scenarios() -> None:
    scenarios = comparison.build_comparison_scenarios(
        "default", include_full_scan=True, include_computed=False
    )
    full_scan = [
        scenario for scenario in scenarios if scenario.autofit_max_rows is None
    ]
    assert {scenario.rule_autofit_columns for scenario in full_scan} == {"body", "all"}


def test_worksheet_cols_only_normalization_keeps_cell_differences() -> None:
    worksheet = (
        b'<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        b'<cols><col min="1" max="1" width="10"/></cols><sheetData>'
        b'<row r="1"><c r="A1"><v>1</v></c></row></sheetData></worksheet>'
    )
    without_columns, normalization = comparison.benchmark._normalized_zip_member(
        "xl/worksheets/sheet1.xml", worksheet, worksheet_cols_only=True
    )
    assert normalization == "worksheet-cols-only"
    assert b"cols" not in without_columns
    changed_cell = worksheet.replace(b"<v>1</v>", b"<v>2</v>")
    assert (
        without_columns
        != comparison.benchmark._normalized_zip_member(
            "xl/worksheets/sheet1.xml", changed_cell, worksheet_cols_only=True
        )[0]
    )
    assert (
        without_columns
        != comparison.benchmark._normalized_zip_member(
            "xl/worksheets/sheet1.xml",
            worksheet.replace(b'<c r="A1">', b'<c r="A1" s="1">'),
            worksheet_cols_only=True,
        )[0]
    )
    assert (
        without_columns
        != comparison.benchmark._normalized_zip_member(
            "xl/worksheets/sheet1.xml",
            worksheet.replace(b"</sheetData>", b'</sheetData><mergeCells count="1"/>'),
            worksheet_cols_only=True,
        )[0]
    )
    assert (
        comparison.benchmark._normalized_zip_member(
            "xl/styles.xml", b"styles", worksheet_cols_only=True
        )[0]
        == b"styles"
    )


def test_normalized_member_comparison_uses_normalized_size_and_hash() -> None:
    baseline = [
        {
            "name": "xl/worksheets/sheet1.xml",
            "size": 100,
            "comparison_size": 70,
            "comparison_sha256": "same-cells",
            "normalization": "worksheet-cols-only",
        }
    ]
    candidate = [{**baseline[0], "size": 140}]
    assert comparison._member_manifest_equal(baseline, candidate)
    candidate[0]["comparison_sha256"] = "changed-cell"
    assert not comparison._member_manifest_equal(baseline, candidate)


def _record(
    scenario: Any,
    *,
    pair: int,
    variant: str,
    total: float,
    manifest_hash: str = "same",
) -> dict[str, object]:
    return {
        "status": "ok",
        "result": {
            "variant": variant,
            "warmup": False,
            "pair_id": f"pair-{pair}",
            "scenario": asdict(scenario),
            "timing": {
                "total_wall_s": total,
                "total_cpu_s": total,
                "write_wall_s": total * 0.6,
                "write_cpu_s": total * 0.6,
                "close_wall_s": total * 0.4,
                "close_cpu_s": total * 0.4,
            },
            "zip_members": [
                {
                    "name": "sheet.xml",
                    "size": 1,
                    "comparison_size": 1,
                    "sha256": manifest_hash,
                    "comparison_sha256": manifest_hash,
                    "normalization": None,
                }
            ],
        },
    }


def test_target_verdict_uses_paired_total_and_exact_zip_members() -> None:
    scenario = comparison.benchmark.XlsxBenchmarkScenario(
        name="target",
        n_rows=10,
        n_numeric_cols=1,
        n_text_cols=0,
        rule_autofit_columns="body",
        input_kind="parquet_lazyframe",
    )
    records = []
    for pair in range(8):
        records.extend(
            [
                _record(scenario, pair=pair, variant="baseline", total=1.0),
                _record(scenario, pair=pair, variant="candidate", total=0.9),
            ]
        )

    result = comparison.analyze_scenario(
        scenario,
        records,
        policy="single-pass",
        analysis_seed=1,
        bootstrap_resamples=100,
        required_samples=8,
    )
    assert result["verdict"] == "passed"
    assert result["zip_members_equal"] is True

    records[-1] = _record(
        scenario,
        pair=7,
        variant="candidate",
        total=0.9,
        manifest_hash="different",
    )
    mismatch = comparison.analyze_scenario(
        scenario,
        records,
        policy="single-pass",
        analysis_seed=1,
        bootstrap_resamples=100,
        required_samples=8,
    )
    assert mismatch["verdict"] == "failed"
    assert mismatch["reason"] == "zip_member_mismatch"


def test_display_autofit_verdict_has_default_and_full_scan_budgets() -> None:
    default = comparison.benchmark.XlsxBenchmarkScenario(
        name="display-default",
        n_rows=10,
        n_numeric_cols=1,
        n_text_cols=0,
        rule_autofit_columns="body",
        input_kind="parquet_lazyframe",
    )
    full_scan = comparison.benchmark.XlsxBenchmarkScenario(
        name="display-full-scan",
        n_rows=10,
        n_numeric_cols=1,
        n_text_cols=0,
        rule_autofit_columns="all",
        input_kind="parquet_lazyframe",
        autofit_max_rows=None,
    )
    records = [
        item
        for scenario, total in ((default, 1.02), (full_scan, 1.08))
        for pair in range(8)
        for item in (
            _record(scenario, pair=pair, variant="baseline", total=1.0),
            _record(scenario, pair=pair, variant="candidate", total=total),
        )
    ]
    for scenario in (default, full_scan):
        result = comparison.analyze_scenario(
            scenario,
            records,
            policy="display-autofit",
            analysis_seed=3,
            bootstrap_resamples=100,
            required_samples=8,
        )
        assert result["verdict"] == "passed"


def test_display_autofit_verdict_rejects_default_regression() -> None:
    scenario = comparison.benchmark.XlsxBenchmarkScenario(
        name="display-regression",
        n_rows=10,
        n_numeric_cols=1,
        n_text_cols=0,
        rule_autofit_columns="body",
        input_kind="parquet_lazyframe",
    )
    records = [
        item
        for pair in range(8)
        for item in (
            _record(scenario, pair=pair, variant="baseline", total=1.0),
            _record(scenario, pair=pair, variant="candidate", total=1.05),
        )
    ]
    result = comparison.analyze_scenario(
        scenario,
        records,
        policy="display-autofit",
        analysis_seed=4,
        bootstrap_resamples=100,
        required_samples=8,
    )
    assert result["verdict"] == "failed"
    assert result["reason"] == "display_regression"


def test_comparison_rejects_mixed_output_contracts() -> None:
    scenario = comparison.benchmark.XlsxBenchmarkScenario(
        name="mixed-contract",
        n_rows=10,
        n_numeric_cols=1,
        n_text_cols=0,
        rule_autofit_columns="header",
        input_kind="parquet_lazyframe",
    )
    records = []
    for pair in range(8):
        baseline = _record(scenario, pair=pair, variant="baseline", total=1.0)
        candidate = _record(scenario, pair=pair, variant="candidate", total=1.0)
        candidate["result"]["output_contract"] = "worksheet-cols-only"  # type: ignore[index]
        records.extend((baseline, candidate))
    result = comparison.analyze_scenario(
        scenario,
        records,
        policy="display-autofit",
        analysis_seed=5,
        bootstrap_resamples=100,
        required_samples=8,
    )
    assert result["verdict"] == "failed"
    assert result["reason"] == "output_contract_mismatch"


def test_comparison_rejects_duplicate_pair_records() -> None:
    scenario = comparison.benchmark.XlsxBenchmarkScenario(
        name="duplicate-pair",
        n_rows=10,
        n_numeric_cols=1,
        n_text_cols=0,
        rule_autofit_columns="body",
        input_kind="parquet_lazyframe",
    )
    records = [
        _record(scenario, pair=0, variant="baseline", total=1.0),
        _record(scenario, pair=0, variant="baseline", total=1.0),
        _record(scenario, pair=0, variant="candidate", total=1.0),
    ]
    result = comparison.analyze_scenario(
        scenario,
        records,
        policy="display-autofit",
        analysis_seed=6,
        bootstrap_resamples=100,
        required_samples=1,
    )
    assert result["verdict"] == "failed"
    assert result["reason"] == "duplicate_pair_id"


def test_zlib_gate_requires_five_percent_total_and_write_guard() -> None:
    scenario = comparison.benchmark.XlsxBenchmarkScenario(
        name="zlib",
        n_rows=10,
        n_numeric_cols=1,
        n_text_cols=0,
        rule_autofit_columns="all",
        input_kind="parquet_lazyframe",
    )
    records = []
    for pair in range(8):
        records.extend(
            [
                _record(scenario, pair=pair, variant="baseline", total=1.0),
                _record(scenario, pair=pair, variant="candidate", total=0.94),
            ]
        )
    result = comparison.analyze_scenario(
        scenario,
        records,
        policy="zlib",
        analysis_seed=2,
        bootstrap_resamples=100,
        required_samples=8,
    )
    assert result["verdict"] == "passed"


def test_zlib_gate_rejects_confident_regression() -> None:
    scenario = comparison.benchmark.XlsxBenchmarkScenario(
        name="zlib-regression",
        n_rows=10,
        n_numeric_cols=1,
        n_text_cols=0,
        rule_autofit_columns="all",
        input_kind="parquet_lazyframe",
    )
    records = []
    for pair in range(8):
        records.extend(
            [
                _record(scenario, pair=pair, variant="baseline", total=1.0),
                _record(scenario, pair=pair, variant="candidate", total=1.2),
            ]
        )
    result = comparison.analyze_scenario(
        scenario,
        records,
        policy="zlib",
        analysis_seed=2,
        bootstrap_resamples=100,
        required_samples=8,
    )
    assert result["verdict"] == "rejected"
    assert result["reason"] == "zlib_regression"
