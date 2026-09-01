from __future__ import annotations

import importlib.util
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
