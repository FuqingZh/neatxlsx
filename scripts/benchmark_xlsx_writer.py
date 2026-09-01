from __future__ import annotations

import argparse
import importlib
import json
import platform
import re
import statistics
import sys
import tempfile
import zipfile
from collections.abc import Callable
from dataclasses import asdict, dataclass, replace
from datetime import UTC, datetime
from importlib import metadata
from pathlib import Path
from time import perf_counter
from typing import Any, Literal, cast

import polars as pl

PROJECT_ROOT = Path(__file__).resolve().parents[1]
SRC_DIR = PROJECT_ROOT / "python"
if str(SRC_DIR) not in sys.path:
    sys.path.insert(0, str(SRC_DIR))

from neatxlsx import Autofit, Workbook  # noqa: E402

BenchmarkInput = Literal["dataframe", "parquet_lazyframe"]
BenchmarkData = pl.DataFrame | pl.LazyFrame


@dataclass(frozen=True)
class XlsxBenchmarkScenario:
    name: str
    n_rows: int
    n_numeric_cols: int
    n_text_cols: int
    should_autofit_columns: bool = True
    rule_autofit_columns: str = "header"
    input_kind: BenchmarkInput = "dataframe"
    chunk_size: int | None = 8_192


@dataclass(frozen=True)
class XlsxBenchmarkStats:
    backend: str
    scenario: XlsxBenchmarkScenario
    n_cols: int
    repeats: int
    warmup_runs: int
    times_seconds: list[float]
    mean_seconds: float
    median_seconds: float
    min_seconds: float
    max_seconds: float
    stdev_seconds: float
    output_size_bytes_mean: int


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run XLSX write performance benchmarks for neatxlsx.Workbook.",
    )
    parser.add_argument(
        "--repeat",
        type=int,
        default=5,
        help="Number of measured runs for each scenario.",
    )
    parser.add_argument(
        "--warmup",
        type=int,
        default=1,
        help="Number of warmup runs for each scenario.",
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=PROJECT_ROOT / "benchmarks" / "xlsx_writer" / "results",
        help="Directory where benchmark result files are written.",
    )
    parser.add_argument(
        "--profile",
        choices=("default", "huge"),
        default="default",
        help="Scenario profile to run.",
    )
    parser.add_argument(
        "--backend",
        choices=("rust",),
        default="rust",
        help="Backend implementation to benchmark.",
    )
    parser.add_argument(
        "--chunk-size",
        type=int,
        default=None,
        help=(
            "Override the scenario batch size. By default scenarios use 8192 rows; "
            "omit this option to retain that recorded default."
        ),
    )
    return parser.parse_args()


def build_scenarios(profile: str) -> list[XlsxBenchmarkScenario]:
    if profile == "default":
        return [
            XlsxBenchmarkScenario(
                name="narrow_tall_dataframe_header",
                n_rows=40_000,
                n_numeric_cols=8,
                n_text_cols=3,
                should_autofit_columns=True,
                rule_autofit_columns="header",
            ),
            XlsxBenchmarkScenario(
                name="wide_medium_dataframe_all",
                n_rows=10_000,
                n_numeric_cols=24,
                n_text_cols=12,
                should_autofit_columns=True,
                rule_autofit_columns="all",
            ),
            XlsxBenchmarkScenario(
                name="narrow_tall_parquet_lazyframe_none",
                n_rows=40_000,
                n_numeric_cols=8,
                n_text_cols=3,
                should_autofit_columns=False,
                rule_autofit_columns="none",
                input_kind="parquet_lazyframe",
            ),
            XlsxBenchmarkScenario(
                name="narrow_tall_parquet_lazyframe_header",
                n_rows=40_000,
                n_numeric_cols=8,
                n_text_cols=3,
                should_autofit_columns=True,
                rule_autofit_columns="header",
                input_kind="parquet_lazyframe",
            ),
            XlsxBenchmarkScenario(
                name="wide_medium_parquet_lazyframe_body",
                n_rows=10_000,
                n_numeric_cols=24,
                n_text_cols=12,
                should_autofit_columns=True,
                rule_autofit_columns="body",
                input_kind="parquet_lazyframe",
            ),
            XlsxBenchmarkScenario(
                name="wide_medium_parquet_lazyframe_all",
                n_rows=10_000,
                n_numeric_cols=24,
                n_text_cols=12,
                should_autofit_columns=True,
                rule_autofit_columns="all",
                input_kind="parquet_lazyframe",
            ),
        ]

    return [
        XlsxBenchmarkScenario(
            name="huge_tall_dataframe_header",
            n_rows=250_000,
            n_numeric_cols=10,
            n_text_cols=4,
            should_autofit_columns=True,
            rule_autofit_columns="header",
        ),
        XlsxBenchmarkScenario(
            name="huge_wide_dataframe_all",
            n_rows=50_000,
            n_numeric_cols=40,
            n_text_cols=20,
            should_autofit_columns=True,
            rule_autofit_columns="all",
        ),
        XlsxBenchmarkScenario(
            name="huge_tall_parquet_lazyframe_none",
            n_rows=250_000,
            n_numeric_cols=10,
            n_text_cols=4,
            should_autofit_columns=False,
            rule_autofit_columns="none",
            input_kind="parquet_lazyframe",
        ),
        XlsxBenchmarkScenario(
            name="huge_tall_parquet_lazyframe_header",
            n_rows=250_000,
            n_numeric_cols=10,
            n_text_cols=4,
            should_autofit_columns=True,
            rule_autofit_columns="header",
            input_kind="parquet_lazyframe",
        ),
        XlsxBenchmarkScenario(
            name="huge_wide_parquet_lazyframe_body",
            n_rows=50_000,
            n_numeric_cols=40,
            n_text_cols=20,
            should_autofit_columns=True,
            rule_autofit_columns="body",
            input_kind="parquet_lazyframe",
        ),
        XlsxBenchmarkScenario(
            name="huge_wide_parquet_lazyframe_all",
            n_rows=50_000,
            n_numeric_cols=40,
            n_text_cols=20,
            should_autofit_columns=True,
            rule_autofit_columns="all",
            input_kind="parquet_lazyframe",
        ),
    ]


def detect_neatxlsx_version() -> str:
    try:
        return metadata.version("neatxlsx")
    except metadata.PackageNotFoundError:
        return "local-src"


def resolve_rust_backend_build() -> tuple[Path, str]:
    module_rs = importlib.import_module("neatxlsx._native")
    path_file_module = getattr(module_rs, "__file__", None)
    if not path_file_module:
        raise RuntimeError("Cannot resolve Rust backend shared library path.")
    c_profile_build = getattr(module_rs, "__build_profile__", None)
    if not isinstance(c_profile_build, str) or not c_profile_build:
        raise RuntimeError(
            "Rust backend does not report its Cargo build profile. "
            "Rebuild with `pdm run develop --release`."
        )
    return Path(path_file_module).resolve(), c_profile_build


def resolve_rust_backend_binary_path() -> Path:
    path_file_backend, _ = resolve_rust_backend_build()
    return path_file_backend


def enforce_release_rust_backend() -> Path:
    path_file_backend, c_profile_build = resolve_rust_backend_build()
    if c_profile_build != "release":
        raise RuntimeError(
            "Benchmark requires Cargo release profile, but current Rust backend "
            f"at {path_file_backend} reports {c_profile_build!r}. "
            "Rebuild with `pdm run develop --release`."
        )

    return path_file_backend


def derive_excel_col_name_from_idx_1based(n_idx_col_1based: int) -> str:
    if n_idx_col_1based < 1:
        raise ValueError("Column index must be >= 1")

    l_chr_col = []
    n_idx_col = n_idx_col_1based
    while n_idx_col > 0:
        n_idx_col, n_rem = divmod(n_idx_col - 1, 26)
        l_chr_col.append(chr(ord("A") + n_rem))
    return "".join(reversed(l_chr_col))


def derive_excel_col_idx_1based_from_name(c_col_name: str) -> int:
    n_idx_col = 0
    for ch in c_col_name:
        if not ("A" <= ch <= "Z"):
            raise ValueError(f"Invalid Excel column name: {c_col_name!r}")
        n_idx_col = n_idx_col * 26 + (ord(ch) - ord("A") + 1)
    return n_idx_col


def derive_expected_dimension_ref(*, n_rows_total: int, n_cols_total: int) -> str:
    n_rows_final = max(1, n_rows_total)
    n_cols_final = max(1, n_cols_total)
    c_cell_end = f"{derive_excel_col_name_from_idx_1based(n_cols_final)}{n_rows_final}"
    if c_cell_end == "A1":
        return "A1"
    return f"A1:{c_cell_end}"


def parse_dimension_ref(c_dimension_ref: str) -> tuple[int, int]:
    c_ref_end = c_dimension_ref.split(":")[-1]
    m_ref = re.fullmatch(r"([A-Z]+)([0-9]+)", c_ref_end)
    if not m_ref:
        raise ValueError(f"Invalid worksheet dimension ref: {c_dimension_ref!r}")

    c_col_name, c_row_idx = m_ref.groups()
    n_cols = derive_excel_col_idx_1based_from_name(c_col_name)
    n_rows = int(c_row_idx)
    return n_rows, n_cols


def validate_xlsx_output(
    *,
    path_xlsx_out: Path,
    expected_rows_total: int,
    expected_cols_total: int,
) -> None:
    with zipfile.ZipFile(path_xlsx_out) as zf:
        v_xml_sheet = zf.read("xl/worksheets/sheet1.xml")

    m_dimension = re.search(rb'<dimension[^>]*\sref="([^"]+)"', v_xml_sheet)
    if not m_dimension:
        raise ValueError("Missing worksheet dimension in `sheet1.xml`.")
    c_dimension_ref = m_dimension.group(1).decode("ascii")

    n_rows_from_dimension, n_cols_from_dimension = parse_dimension_ref(c_dimension_ref)
    n_rows_from_tags = v_xml_sheet.count(b"<row ")

    c_expected_dimension = derive_expected_dimension_ref(
        n_rows_total=expected_rows_total,
        n_cols_total=expected_cols_total,
    )

    if c_dimension_ref != c_expected_dimension:
        raise ValueError(
            f"Dimension mismatch: expected={c_expected_dimension}, got={c_dimension_ref}."
        )
    if n_rows_from_dimension != expected_rows_total:
        raise ValueError(
            "Dimension row mismatch: "
            f"expected={expected_rows_total}, got={n_rows_from_dimension}."
        )
    if n_cols_from_dimension != expected_cols_total:
        raise ValueError(
            "Dimension column mismatch: "
            f"expected={expected_cols_total}, got={n_cols_from_dimension}."
        )
    if n_rows_from_tags != expected_rows_total:
        raise ValueError(
            f"<row> tag count mismatch: expected={expected_rows_total}, got={n_rows_from_tags}."
        )


def build_dataframe(
    *, n_rows: int, n_numeric_cols: int, n_text_cols: int
) -> pl.DataFrame:
    df = pl.DataFrame({"row_id": pl.Series("row_id", range(n_rows), dtype=pl.Int64)})

    l_expr: list[pl.Expr] = []
    for n_idx in range(n_numeric_cols):
        l_expr.append(
            ((pl.col("row_id") * (n_idx + 1)).cast(pl.Float64) / 7.0).alias(
                f"value_{n_idx:02d}"
            )
        )
    for n_idx in range(n_text_cols):
        l_expr.append(
            (
                pl.lit(f"group_{n_idx:02d}_")
                + (pl.col("row_id") % 10_000).cast(pl.String)
            ).alias(f"text_{n_idx:02d}")
        )

    if l_expr:
        df = df.with_columns(l_expr)

    if n_numeric_cols > 0:
        df = df.with_columns(
            pl.when((pl.col("row_id") % 113) == 0)
            .then(None)
            .otherwise(pl.col("value_00"))
            .alias("value_00")
        )

    return df


def make_parquet_lazyframe(path_parquet: Path) -> pl.LazyFrame:
    """Build the timed LazyFrame plan from a pre-generated deterministic fixture."""
    return (
        pl.scan_parquet(path_parquet)
        .filter(pl.col("row_id") >= 0)
        .with_columns((pl.col("row_id") + 0).alias("row_id"))
        .select(pl.all())
    )


def build_data_factory(
    *,
    scenario: XlsxBenchmarkScenario,
    path_dir_tmp: Path,
) -> Callable[[], BenchmarkData]:
    df = build_dataframe(
        n_rows=scenario.n_rows,
        n_numeric_cols=scenario.n_numeric_cols,
        n_text_cols=scenario.n_text_cols,
    )
    if scenario.input_kind == "dataframe":
        return lambda: df

    path_parquet = path_dir_tmp / f"fixture_{scenario.name}.parquet"
    df.write_parquet(path_parquet)
    return lambda: make_parquet_lazyframe(path_parquet)


def run_one_write(
    *,
    writer_cls: Any,
    make_data: Callable[[], BenchmarkData],
    path_xlsx_out: Path,
    should_autofit_columns: bool,
    rule_autofit_columns: str,
    chunk_size: int | None,
) -> float:
    n_t_start = perf_counter()
    data = make_data()
    with writer_cls(path_xlsx_out, chunk_size=chunk_size) as inst_writer:
        mode = cast(
            Literal["none", "header", "body", "all"],
            rule_autofit_columns if should_autofit_columns else "none",
        )
        autofit = Autofit(mode=mode)
        inst_writer.write_sheet(
            data=data,
            sheet_name="benchmark",
            autofit=autofit,
        )
    return perf_counter() - n_t_start


def benchmark_scenario(
    *,
    backend: str,
    writer_cls: Any,
    scenario: XlsxBenchmarkScenario,
    repeat: int,
    warmup: int,
    path_dir_tmp: Path,
    chunk_size: int | None,
) -> XlsxBenchmarkStats:
    make_data = build_data_factory(
        scenario=scenario,
        path_dir_tmp=path_dir_tmp,
    )

    for n_idx in range(warmup):
        path_file_out = path_dir_tmp / f"{backend}_{scenario.name}_warmup_{n_idx}.xlsx"
        run_one_write(
            writer_cls=writer_cls,
            make_data=make_data,
            path_xlsx_out=path_file_out,
            should_autofit_columns=scenario.should_autofit_columns,
            rule_autofit_columns=scenario.rule_autofit_columns,
            chunk_size=chunk_size,
        )
        validate_xlsx_output(
            path_xlsx_out=path_file_out,
            expected_rows_total=scenario.n_rows + 1,
            expected_cols_total=scenario.n_numeric_cols + scenario.n_text_cols + 1,
        )
        path_file_out.unlink(missing_ok=True)

    l_times_seconds: list[float] = []
    l_output_size_bytes: list[int] = []

    for n_idx in range(repeat):
        path_file_out = path_dir_tmp / f"{backend}_{scenario.name}_{n_idx}.xlsx"
        n_elapsed = run_one_write(
            writer_cls=writer_cls,
            make_data=make_data,
            path_xlsx_out=path_file_out,
            should_autofit_columns=scenario.should_autofit_columns,
            rule_autofit_columns=scenario.rule_autofit_columns,
            chunk_size=chunk_size,
        )
        validate_xlsx_output(
            path_xlsx_out=path_file_out,
            expected_rows_total=scenario.n_rows + 1,
            expected_cols_total=scenario.n_numeric_cols + scenario.n_text_cols + 1,
        )
        l_times_seconds.append(n_elapsed)
        l_output_size_bytes.append(path_file_out.stat().st_size)
        path_file_out.unlink(missing_ok=True)

    n_stdev_seconds = (
        statistics.stdev(l_times_seconds) if len(l_times_seconds) > 1 else 0.0
    )

    return XlsxBenchmarkStats(
        backend=backend,
        scenario=scenario,
        n_cols=scenario.n_numeric_cols + scenario.n_text_cols + 1,
        repeats=repeat,
        warmup_runs=warmup,
        times_seconds=l_times_seconds,
        mean_seconds=statistics.mean(l_times_seconds),
        median_seconds=statistics.median(l_times_seconds),
        min_seconds=min(l_times_seconds),
        max_seconds=max(l_times_seconds),
        stdev_seconds=n_stdev_seconds,
        output_size_bytes_mean=round(statistics.mean(l_output_size_bytes)),
    )


def render_markdown_summary(payload: dict[str, Any]) -> str:
    l_scenarios = payload["scenarios"]
    assert isinstance(l_scenarios, list)

    l_lines = [
        "# XLSX Benchmark Record",
        "",
        f"- Timestamp (UTC): `{payload['timestamp_utc']}`",
        f"- Command: `{payload['command']}`",
        f"- Platform: `{payload['platform']}`",
        f"- Python: `{payload['python_version']}`",
        f"- Rust backend binary: `{payload['rust_backend_binary']}`",
        f"- Validation: `{payload['validation_policy']}`",
        "- Package versions:",
        f"  - `neatxlsx`: `{payload['packages']['neatxlsx']}`",
        f"  - `polars`: `{payload['packages']['polars']}`",
        "",
        "| backend | scenario | input | rows | cols | chunk | autofit | repeat | median_s | mean_s | min_s | max_s | stdev_s | mean_size_mb |",
        "| --- | --- | --- | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]

    for item in l_scenarios:
        assert isinstance(item, dict)
        cfg = item["scenario"]
        assert isinstance(cfg, dict)

        n_size_mb = float(item["output_size_bytes_mean"]) / (1024 * 1024)
        l_lines.append(
            "| "
            f"{item['backend']} | {cfg['name']} | {cfg['input_kind']} | "
            f"{cfg['n_rows']} | {item['n_cols']} | {cfg['chunk_size']} | "
            f"{cfg['should_autofit_columns']} ({cfg['rule_autofit_columns']}) | "
            f"{item['repeats']} | {item['median_seconds']:.3f} | "
            f"{item['mean_seconds']:.3f} | {item['min_seconds']:.3f} | "
            f"{item['max_seconds']:.3f} | {item['stdev_seconds']:.3f} | {n_size_mb:.2f} |"
        )

    l_lines.extend(
        [
            "",
            "## Raw Timings (seconds)",
            "",
        ]
    )

    for item in l_scenarios:
        assert isinstance(item, dict)
        cfg = item["scenario"]
        assert isinstance(cfg, dict)
        l_lines.append(
            f"- `{item['backend']} / {cfg['name']}`: `{item['times_seconds']}`"
        )

    return "\n".join(l_lines) + "\n"


def main() -> int:
    args = parse_args()
    if args.repeat < 1:
        raise ValueError("--repeat must be >= 1")
    if args.warmup < 0:
        raise ValueError("--warmup must be >= 0")
    if args.chunk_size is not None and args.chunk_size < 1:
        raise ValueError("--chunk-size must be >= 1 when provided")

    l_scenarios = build_scenarios(args.profile)
    if args.chunk_size is not None:
        l_scenarios = [
            replace(scenario, chunk_size=args.chunk_size) for scenario in l_scenarios
        ]
    l_backends: list[tuple[str, Any]] = [("rust", Workbook)]
    path_file_backend_rs = enforce_release_rust_backend()

    args.out_dir.mkdir(parents=True, exist_ok=True)
    ts = datetime.now(UTC)
    c_timestamp_compact = ts.strftime("%Y%m%dT%H%M%SZ")

    with tempfile.TemporaryDirectory(prefix="neatxlsx_bench_") as c_dir_tmp:
        path_dir_tmp = Path(c_dir_tmp)
        l_stats = []
        for c_backend, cls_writer in l_backends:
            for cfg_scenario in l_scenarios:
                l_stats.append(
                    benchmark_scenario(
                        backend=c_backend,
                        writer_cls=cls_writer,
                        scenario=cfg_scenario,
                        repeat=args.repeat,
                        warmup=args.warmup,
                        path_dir_tmp=path_dir_tmp,
                        chunk_size=cfg_scenario.chunk_size,
                    )
                )

    payload = {
        "timestamp_utc": ts.isoformat(),
        "command": " ".join(sys.argv),
        "platform": platform.platform(),
        "python_version": sys.version.split()[0],
        "packages": {
            "neatxlsx": detect_neatxlsx_version(),
            "polars": pl.__version__,
        },
        "rust_backend_binary": str(path_file_backend_rs),
        "validation_policy": "enforce_release_backend + validate sheet1 dimension/rows/cols",
        "input_timing_policy": (
            "DataFrame construction and Parquet fixture generation are excluded. "
            "Parquet LazyFrame timing starts before scan_parquet/filter/expression/"
            "projection plan construction and ends after XLSX save."
        ),
        "repeat": args.repeat,
        "warmup": args.warmup,
        "profile": args.profile,
        "backend": args.backend,
        "scenarios": [asdict(item) for item in l_stats],
    }

    path_file_json = args.out_dir / f"xlsx_writer_{c_timestamp_compact}.json"
    path_file_md = args.out_dir / f"xlsx_writer_{c_timestamp_compact}.md"

    path_file_json.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    path_file_md.write_text(render_markdown_summary(payload), encoding="utf-8")

    print(path_file_json)
    print(path_file_md)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
