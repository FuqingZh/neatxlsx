from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import statistics
import subprocess
import sys
from dataclasses import asdict, dataclass, replace
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, Literal

import benchmark_xlsx_writer as benchmark

Variant = Literal["baseline", "candidate"]
VerdictPolicy = Literal["single-pass", "zlib"]


@dataclass(frozen=True)
class VariantConfig:
    label: Variant
    checkout_root: Path
    python: Path


@dataclass(frozen=True)
class PlannedRun:
    run_id: str
    variant: Variant
    warmup: bool
    block: int | None
    position: int | None
    pair_id: str | None
    scenario_name: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare two isolated neatxlsx builds in balanced fresh processes."
    )
    parser.add_argument("--baseline-root", type=Path, required=True)
    parser.add_argument("--baseline-python", type=Path, required=True)
    parser.add_argument("--baseline-source-id")
    parser.add_argument("--candidate-root", type=Path, required=True)
    parser.add_argument("--candidate-python", type=Path, required=True)
    parser.add_argument("--candidate-source-id")
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--profile", choices=("default", "huge"), default="huge")
    parser.add_argument(
        "--scenario",
        action="append",
        dest="scenarios",
        help="Run only this scenario name; may be repeated.",
    )
    parser.add_argument("--blocks", type=int, default=4)
    parser.add_argument("--warmup", type=int, choices=(0, 1), default=1)
    parser.add_argument("--order-seed", type=int, default=20260901)
    parser.add_argument("--analysis-seed", type=int, default=20260902)
    parser.add_argument("--bootstrap-resamples", type=int, default=10_000)
    parser.add_argument("--cpu-list")
    parser.add_argument("--polars-threads", type=int, default=4)
    parser.add_argument("--timeout-seconds", type=float, default=900.0)
    parser.add_argument("--include-full-scan", action="store_true")
    parser.add_argument("--include-computed", action="store_true")
    parser.add_argument(
        "--verdict-policy",
        choices=("single-pass", "zlib"),
        default="single-pass",
    )
    parser.add_argument("--allow-identical-native", action="store_true")
    parser.add_argument("--allow-dirty-source", action="store_true")
    parser.add_argument("--keep-workbooks", action="store_true")
    return parser.parse_args()


def _is_relative_to(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
    except ValueError:
        return False
    return True


def paths_overlap(left: Path, right: Path) -> bool:
    left = left.resolve()
    right = right.resolve()
    return _is_relative_to(left, right) or _is_relative_to(right, left)


def validate_comparison_paths(
    variants: tuple[VariantConfig, VariantConfig], out_dir: Path
) -> tuple[VariantConfig, VariantConfig]:
    resolved: list[VariantConfig] = []
    for variant in variants:
        root = variant.checkout_root.resolve()
        python = variant.python.absolute()
        if not (root / "python" / "neatxlsx").is_dir():
            raise ValueError(f"{variant.label} root lacks python/neatxlsx: {root}")
        if not python.is_file():
            raise ValueError(f"{variant.label} Python does not exist: {python}")
        resolved.append(VariantConfig(variant.label, root, python))
    output = out_dir.resolve()
    for variant in resolved:
        if paths_overlap(output, variant.checkout_root):
            raise ValueError(
                f"Output directory must not overlap {variant.label} root: {output}"
            )
    if resolved[0].checkout_root == resolved[1].checkout_root:
        raise ValueError("Baseline and candidate roots must be distinct.")
    return resolved[0], resolved[1]


def build_comparison_scenarios(
    profile: str,
    *,
    include_full_scan: bool,
    include_computed: bool,
) -> list[benchmark.XlsxBenchmarkScenario]:
    scenarios = benchmark.build_scenarios(profile)
    all_lazy = next(
        scenario
        for scenario in scenarios
        if scenario.input_kind == "parquet_lazyframe"
        and scenario.rule_autofit_columns == "all"
    )
    if include_full_scan:
        scenarios.append(
            replace(
                all_lazy,
                name=f"{all_lazy.name}_full_scan",
                autofit_max_rows=None,
            )
        )
    if include_computed:
        scenarios.append(
            replace(
                all_lazy,
                name=f"{all_lazy.name}_computed_full_scan",
                input_kind="computed_parquet_lazyframe",
                autofit_max_rows=None,
            )
        )
    return scenarios


def _scenario_seed(seed: int, scenario_name: str) -> int:
    digest = hashlib.sha256(f"{seed}:{scenario_name}".encode()).digest()
    return int.from_bytes(digest[:8], "big")


def generate_run_plan(
    scenario_name: str,
    *,
    blocks: int,
    warmup: bool,
    seed: int,
) -> list[PlannedRun]:
    if blocks < 1:
        raise ValueError("blocks must be >= 1")
    rng = random.Random(_scenario_seed(seed, scenario_name))
    plan: list[PlannedRun] = []
    if warmup:
        warmup_order: tuple[Variant, Variant] = (
            ("baseline", "candidate")
            if rng.randrange(2) == 0
            else ("candidate", "baseline")
        )
        for position, variant in enumerate(warmup_order):
            plan.append(
                PlannedRun(
                    run_id=f"{scenario_name}-warmup-{position}-{variant}",
                    variant=variant,
                    warmup=True,
                    block=None,
                    position=position,
                    pair_id=None,
                    scenario_name=scenario_name,
                )
            )

    for block in range(blocks):
        order: tuple[Variant, Variant, Variant, Variant] = (
            ("baseline", "candidate", "candidate", "baseline")
            if rng.randrange(2) == 0
            else ("candidate", "baseline", "baseline", "candidate")
        )
        for position, variant in enumerate(order):
            pair = position // 2
            plan.append(
                PlannedRun(
                    run_id=(
                        f"{scenario_name}-block-{block}-position-{position}-{variant}"
                    ),
                    variant=variant,
                    warmup=False,
                    block=block,
                    position=position,
                    pair_id=f"{scenario_name}-block-{block}-pair-{pair}",
                    scenario_name=scenario_name,
                )
            )
    return plan


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _git_output(root: Path, *arguments: str) -> str | None:
    completed = subprocess.run(
        ["git", "-C", str(root), *arguments],
        text=True,
        capture_output=True,
        check=False,
    )
    return completed.stdout.strip() if completed.returncode == 0 else None


def _root_metadata(
    variant: VariantConfig,
    declared_source_id: str | None,
    *,
    allow_dirty: bool,
) -> dict[str, Any]:
    lock_path = variant.checkout_root / "Cargo.lock"
    git_sha = _git_output(variant.checkout_root, "rev-parse", "HEAD")
    source_id = declared_source_id or git_sha
    if not source_id:
        raise ValueError(
            f"{variant.label} source has no Git metadata; pass "
            f"--{variant.label}-source-id."
        )
    git_status = _git_output(variant.checkout_root, "status", "--short")
    if git_status and not allow_dirty:
        raise ValueError(
            f"{variant.label} source is dirty; commit it or pass "
            "--allow-dirty-source for a non-acceptance smoke run."
        )
    return {
        "label": variant.label,
        "checkout_root": str(variant.checkout_root),
        "python": str(variant.python),
        "source_id": source_id,
        "git_sha": git_sha,
        "git_status_short": git_status,
        "cargo_lock_sha256": _sha256_file(lock_path) if lock_path.is_file() else None,
    }


def _fixture_key(scenario: benchmark.XlsxBenchmarkScenario) -> tuple[int, int, int]:
    return scenario.n_rows, scenario.n_numeric_cols, scenario.n_text_cols


def create_fixtures(
    scenarios: list[benchmark.XlsxBenchmarkScenario], fixture_dir: Path
) -> dict[tuple[int, int, int], Path]:
    fixture_dir.mkdir(parents=True)
    paths: dict[tuple[int, int, int], Path] = {}
    for scenario in scenarios:
        if scenario.input_kind == "dataframe":
            continue
        key = _fixture_key(scenario)
        if key in paths:
            continue
        path = fixture_dir / f"rows-{key[0]}-numeric-{key[1]}-text-{key[2]}.parquet"
        data = benchmark.build_dataframe(
            n_rows=key[0], n_numeric_cols=key[1], n_text_cols=key[2]
        )
        data.write_parquet(path)
        paths[key] = path
    return paths


def _write_json(path: Path, value: Any) -> None:
    path.write_text(
        json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def _append_jsonl(path: Path, value: Any) -> None:
    with path.open("a", encoding="utf-8") as stream:
        stream.write(json.dumps(value, ensure_ascii=False, sort_keys=True) + "\n")
        stream.flush()
        os.fsync(stream.fileno())


def invoke_worker(
    *,
    variant: VariantConfig,
    run: PlannedRun,
    scenario: benchmark.XlsxBenchmarkScenario,
    fixture_path: Path | None,
    result_dir: Path,
    cpu_list: str | None,
    polars_threads: int,
    timeout_seconds: float,
    keep_workbook: bool,
) -> dict[str, Any]:
    config_dir = result_dir / "configs"
    worker_dir = result_dir / "workers"
    workbook_dir = result_dir / "workbooks"
    for directory in (config_dir, worker_dir, workbook_dir):
        directory.mkdir(exist_ok=True)
    config_path = config_dir / f"{run.run_id}.json"
    worker_result_path = worker_dir / f"{run.run_id}.json"
    workbook_path = workbook_dir / f"{run.run_id}.xlsx"
    config = {
        **asdict(run),
        "checkout_root": str(variant.checkout_root),
        "scenario": asdict(scenario),
        "fixture_path": str(fixture_path) if fixture_path else None,
        "output_path": str(workbook_path),
        "result_path": str(worker_result_path),
    }
    _write_json(config_path, config)

    env = os.environ.copy()
    env[benchmark.CHECKOUT_ROOT_ENV] = str(variant.checkout_root)
    env["POLARS_MAX_THREADS"] = str(polars_threads)
    if cpu_list is not None:
        env[benchmark.CPU_LIST_ENV] = cpu_list
    else:
        env.pop(benchmark.CPU_LIST_ENV, None)
    command = [
        str(variant.python),
        str(Path(benchmark.__file__).resolve()),
        "--single-run-config",
        str(config_path),
    ]
    started = datetime.now(UTC).isoformat()
    try:
        completed = subprocess.run(
            command,
            env=env,
            text=True,
            capture_output=True,
            timeout=timeout_seconds,
            check=False,
        )
    except subprocess.TimeoutExpired as exc:
        return {
            "status": "failed",
            "run": asdict(run),
            "started_utc": started,
            "error": "timeout",
            "timeout_seconds": timeout_seconds,
            "stdout": exc.stdout,
            "stderr": exc.stderr,
        }
    if completed.returncode != 0 or not worker_result_path.is_file():
        return {
            "status": "failed",
            "run": asdict(run),
            "started_utc": started,
            "error": "worker_failed",
            "returncode": completed.returncode,
            "stdout": completed.stdout,
            "stderr": completed.stderr,
        }
    result = json.loads(worker_result_path.read_text(encoding="utf-8"))
    if result["run_id"] != run.run_id or result["variant"] != variant.label:
        raise RuntimeError(f"Worker identity mismatch for {run.run_id}.")
    if result["scenario"] != asdict(scenario):
        raise RuntimeError(f"Worker scenario mismatch for {run.run_id}.")
    if not keep_workbook:
        workbook_path.unlink(missing_ok=True)
    return {
        "status": "ok",
        "started_utc": started,
        "command": command,
        "result": result,
    }


def _quantile(sorted_values: list[float], probability: float) -> float:
    if not sorted_values:
        raise ValueError("Cannot calculate a quantile of an empty sequence.")
    position = probability * (len(sorted_values) - 1)
    lower = int(position)
    upper = min(lower + 1, len(sorted_values) - 1)
    fraction = position - lower
    return sorted_values[lower] * (1 - fraction) + sorted_values[upper] * fraction


def bootstrap_median_ci(
    ratios: list[float], *, seed: int, resamples: int
) -> tuple[float, float]:
    if not ratios:
        raise ValueError("At least one paired ratio is required.")
    if resamples < 1:
        raise ValueError("bootstrap resamples must be >= 1")
    rng = random.Random(seed)
    medians = sorted(
        statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(resamples)
    )
    return _quantile(medians, 0.025), _quantile(medians, 0.975)


def _member_manifest_equal(
    left: list[dict[str, Any]], right: list[dict[str, Any]]
) -> bool:
    def comparison_view(items: list[dict[str, Any]]) -> list[dict[str, Any]]:
        return [
            {
                "name": item["name"],
                "size": item["size"],
                "comparison_sha256": item["comparison_sha256"],
                "normalization": item["normalization"],
            }
            for item in items
        ]

    return comparison_view(left) == comparison_view(right)


def analyze_scenario(
    scenario: benchmark.XlsxBenchmarkScenario,
    records: list[dict[str, Any]],
    *,
    policy: VerdictPolicy,
    analysis_seed: int,
    bootstrap_resamples: int,
    required_samples: int,
) -> dict[str, Any]:
    successful = [
        item["result"]
        for item in records
        if item["status"] == "ok"
        and not item["result"]["warmup"]
        and item["result"]["scenario"]["name"] == scenario.name
    ]
    by_pair: dict[str, dict[str, dict[str, Any]]] = {}
    for result in successful:
        pair_id = result["pair_id"]
        if not isinstance(pair_id, str):
            continue
        pair = by_pair.setdefault(pair_id, {})
        pair[result["variant"]] = result

    paired = [
        pair for pair in by_pair.values() if set(pair) == {"baseline", "candidate"}
    ]
    manifest_equal = all(
        _member_manifest_equal(
            pair["baseline"]["zip_members"], pair["candidate"]["zip_members"]
        )
        for pair in paired
    )
    metrics: dict[str, Any] = {}
    for metric in (
        "total_wall_s",
        "total_cpu_s",
        "write_wall_s",
        "write_cpu_s",
        "close_wall_s",
        "close_cpu_s",
    ):
        ratios = [
            pair["candidate"]["timing"][metric] / pair["baseline"]["timing"][metric]
            for pair in paired
        ]
        if ratios:
            low, high = bootstrap_median_ci(
                ratios,
                seed=_scenario_seed(analysis_seed, f"{scenario.name}:{metric}"),
                resamples=bootstrap_resamples,
            )
            metrics[metric] = {
                "ratios": ratios,
                "median_ratio": statistics.median(ratios),
                "ci95": [low, high],
            }

    samples_by_variant = {
        variant: sum(result["variant"] == variant for result in successful)
        for variant in ("baseline", "candidate")
    }
    verdict = "inconclusive"
    reason = "insufficient_samples"
    total = metrics.get("total_wall_s")
    write = metrics.get("write_wall_s")
    enough = all(value >= required_samples for value in samples_by_variant.values())
    if not manifest_equal:
        verdict, reason = "failed", "zip_member_mismatch"
    elif enough and total is not None:
        median = total["median_ratio"]
        upper = total["ci95"][1]
        if policy == "zlib":
            write_median = write["median_ratio"] if write else float("inf")
            passed = median <= 0.95 and upper < 1.0 and write_median < 1.03
            verdict = "passed" if passed else "inconclusive"
            reason = "zlib_gate_met" if passed else "zlib_gate_unmet"
        elif scenario.input_kind == "dataframe":
            verdict, reason = "diagnostic", "dataframe_control"
        elif scenario.rule_autofit_columns in {"body", "all"}:
            passed = median <= 0.97 and upper < 1.0
            verdict = "passed" if passed else "inconclusive"
            reason = "target_gate_met" if passed else "target_gate_unmet"
        else:
            passed = upper < 1.03
            verdict = "passed" if passed else "failed"
            reason = "control_gate_met" if passed else "control_regression_not_excluded"

    return {
        "scenario": asdict(scenario),
        "samples_by_variant": samples_by_variant,
        "paired_observations": len(paired),
        "zip_members_equal": manifest_equal,
        "metrics": metrics,
        "verdict": verdict,
        "reason": reason,
    }


def render_markdown(payload: dict[str, Any]) -> str:
    lines = [
        "# Interleaved XLSX Comparison",
        "",
        f"- Status: `{payload['status']}`",
        f"- Timestamp: `{payload['timestamp_utc']}`",
        f"- Verdict policy: `{payload['verdict_policy']}`",
        f"- Order seed: `{payload['order_seed']}`",
        f"- Analysis seed: `{payload['analysis_seed']}`",
        "",
        "| Scenario | Mode | Samples B/C | Median total ratio | 95% CI | ZIP members | Verdict |",
        "| --- | --- | ---: | ---: | ---: | --- | --- |",
    ]
    for item in payload["analysis"]:
        total = item["metrics"].get("total_wall_s")
        ratio = f"{total['median_ratio']:.4f}" if total else "n/a"
        ci = f"[{total['ci95'][0]:.4f}, {total['ci95'][1]:.4f}]" if total else "n/a"
        counts = item["samples_by_variant"]
        lines.append(
            f"| {item['scenario']['name']} | {item['scenario']['rule_autofit_columns']} "
            f"| {counts['baseline']}/{counts['candidate']} | {ratio} | {ci} "
            f"| {item['zip_members_equal']} | {item['verdict']} |"
        )
    lines.extend(
        [
            "",
            "Raw execution records are in `raw.jsonl`; the frozen order is in "
            "`planned.json`.",
        ]
    )
    return "\n".join(lines) + "\n"


def main() -> int:
    args = parse_args()
    if args.blocks < 1:
        raise ValueError("--blocks must be >= 1")
    if args.polars_threads < 1:
        raise ValueError("--polars-threads must be >= 1")
    if args.bootstrap_resamples < 1:
        raise ValueError("--bootstrap-resamples must be >= 1")
    if args.timeout_seconds <= 0:
        raise ValueError("--timeout-seconds must be > 0")
    if args.cpu_list is not None:
        benchmark._parse_cpu_list(args.cpu_list)

    baseline, candidate = validate_comparison_paths(
        (
            VariantConfig("baseline", args.baseline_root, args.baseline_python),
            VariantConfig("candidate", args.candidate_root, args.candidate_python),
        ),
        args.out_dir,
    )
    out_dir = args.out_dir.resolve()
    if out_dir.exists():
        raise FileExistsError(f"Result directory already exists: {out_dir}")
    out_dir.mkdir(parents=True)

    scenarios = build_comparison_scenarios(
        args.profile,
        include_full_scan=args.include_full_scan,
        include_computed=args.include_computed,
    )
    if args.scenarios:
        requested = set(args.scenarios)
        scenarios = [scenario for scenario in scenarios if scenario.name in requested]
        missing = requested - {scenario.name for scenario in scenarios}
        if missing:
            raise ValueError(f"Unknown scenarios: {sorted(missing)}")
    fixtures = create_fixtures(scenarios, out_dir / "fixtures")

    plans = {
        scenario.name: generate_run_plan(
            scenario.name,
            blocks=args.blocks,
            warmup=bool(args.warmup),
            seed=args.order_seed,
        )
        for scenario in scenarios
    }
    planned_payload = {
        "timestamp_utc": datetime.now(UTC).isoformat(),
        "command": sys.argv,
        "harness_path": str(Path(__file__).resolve()),
        "harness_sha256": _sha256_file(Path(__file__).resolve()),
        "worker_path": str(Path(benchmark.__file__).resolve()),
        "worker_sha256": _sha256_file(Path(benchmark.__file__).resolve()),
        "variants": [
            _root_metadata(
                baseline,
                args.baseline_source_id,
                allow_dirty=args.allow_dirty_source,
            ),
            _root_metadata(
                candidate,
                args.candidate_source_id,
                allow_dirty=args.allow_dirty_source,
            ),
        ],
        "profile": args.profile,
        "order_seed": args.order_seed,
        "analysis_seed": args.analysis_seed,
        "blocks": args.blocks,
        "warmup": args.warmup,
        "acceptance_eligible": not (
            args.allow_dirty_source or args.allow_identical_native
        ),
        "non_acceptance_overrides": {
            "allow_dirty_source": args.allow_dirty_source,
            "allow_identical_native": args.allow_identical_native,
        },
        "cpu_list": args.cpu_list,
        "polars_threads": args.polars_threads,
        "scenarios": [asdict(scenario) for scenario in scenarios],
        "fixtures": [
            {"path": str(path), "sha256": _sha256_file(path)}
            for path in fixtures.values()
        ],
        "plans": {name: [asdict(run) for run in plan] for name, plan in plans.items()},
    }
    _write_json(out_dir / "planned.json", planned_payload)

    variants = {"baseline": baseline, "candidate": candidate}
    raw_path = out_dir / "raw.jsonl"
    records: list[dict[str, Any]] = []
    failed = False
    native_hashes: dict[Variant, str] = {}
    for scenario in scenarios:
        fixture = fixtures.get(_fixture_key(scenario))
        for run in plans[scenario.name]:
            record = invoke_worker(
                variant=variants[run.variant],
                run=run,
                scenario=scenario,
                fixture_path=fixture,
                result_dir=out_dir,
                cpu_list=args.cpu_list,
                polars_threads=args.polars_threads,
                timeout_seconds=args.timeout_seconds,
                keep_workbook=args.keep_workbooks,
            )
            _append_jsonl(raw_path, record)
            records.append(record)
            if record["status"] != "ok":
                failed = True
                break
            native_hashes[run.variant] = record["result"]["native"]["native_sha256"]
            if (
                not args.allow_identical_native
                and set(native_hashes) == {"baseline", "candidate"}
                and native_hashes["baseline"] == native_hashes["candidate"]
            ):
                failure = {
                    "status": "failed",
                    "error": "identical_native_hashes",
                    "native_sha256": native_hashes["baseline"],
                }
                _append_jsonl(raw_path, failure)
                records.append(failure)
                failed = True
                break
        if failed:
            break

    required_samples = 8
    analysis = [
        analyze_scenario(
            scenario,
            records,
            policy=args.verdict_policy,
            analysis_seed=args.analysis_seed,
            bootstrap_resamples=args.bootstrap_resamples,
            required_samples=required_samples,
        )
        for scenario in scenarios
    ]
    if not planned_payload["acceptance_eligible"]:
        for item in analysis:
            item["unadjusted_verdict"] = item["verdict"]
            item["unadjusted_reason"] = item["reason"]
            item["verdict"] = "diagnostic"
            item["reason"] = "non_acceptance_override"
    status = "incomplete" if failed else "complete"
    payload = {
        **planned_payload,
        "status": status,
        "verdict_policy": args.verdict_policy,
        "analysis": analysis,
        "raw_record_count": len(records),
    }
    _write_json(out_dir / "comparison.json", payload)
    (out_dir / "comparison.md").write_text(render_markdown(payload), encoding="utf-8")
    print(out_dir / "comparison.json")
    print(out_dir / "comparison.md")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
