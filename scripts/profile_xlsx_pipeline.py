from __future__ import annotations

import argparse
import json
import resource
import sys
import time
import zipfile
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Literal, cast

import polars as pl

PROJECT_ROOT = Path(__file__).resolve().parents[1]
SRC_DIR = PROJECT_ROOT / "python"
if str(SRC_DIR) not in sys.path:
    sys.path.insert(0, str(SRC_DIR))

from neatxlsx import Autofit, Workbook  # noqa: E402
from neatxlsx._rs_bridge import _mod_rs  # noqa: E402

ProfileMode = Literal["collect-only", "arrow-drain", "xlsx-write"]
AutofitMode = Literal["none", "header", "body", "all"]


@dataclass(frozen=True)
class ProfileResult:
    mode: ProfileMode
    rows: int
    cols: int
    sheets: int
    cells: int
    chunk_size: int | None
    elapsed_s: float
    peak_rss_kb: int
    batches: int | None = None
    output_path: str | None = None
    output_size_bytes: int | None = None
    output_size_mb: float | None = None
    has_shared_strings: bool | None = None
    polars: str = pl.__version__


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Profile staged XLSX pipeline costs for LazyFrame inputs.",
    )
    parser.add_argument(
        "--mode",
        choices=("collect-only", "arrow-drain", "xlsx-write"),
        required=True,
    )
    parser.add_argument("--rows", type=int, required=True)
    parser.add_argument("--cols", type=int, default=1400)
    parser.add_argument("--sheets", type=int, default=2)
    parser.add_argument("--chunk-size", type=int, default=None)
    parser.add_argument(
        "--autofit",
        choices=("none", "header", "body", "all"),
        default="header",
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=Path("/tmp/neatxlsx_pipeline_profile"),
    )
    return parser.parse_args()


def make_lf(rows: int, cols: int) -> pl.LazyFrame:
    base = pl.LazyFrame({"row_id": pl.Series("row_id", range(rows), dtype=pl.Int64)})
    return base.select(
        [
            ((pl.col("row_id") * (idx + 1)).cast(pl.Float64) / 7.0).alias(
                f"v_{idx:04d}"
            )
            for idx in range(cols)
        ]
    )


def collect_batches(lf: pl.LazyFrame, chunk_size: int | None) -> Any:
    if chunk_size is None:
        return lf.collect_batches()
    return lf.collect_batches(chunk_size=chunk_size)


def profile_collect_only(
    lf: pl.LazyFrame, *, chunk_size: int | None
) -> tuple[int, int]:
    batches = 0
    rows = 0
    for df in collect_batches(lf, chunk_size):
        batches += 1
        rows += df.height
    return batches, rows


def profile_arrow_drain(lf: pl.LazyFrame, *, chunk_size: int | None) -> Any:
    if _mod_rs is None:
        raise RuntimeError("Rust xlsx backend is unavailable.")
    return _mod_rs._profile_arrow_drain(collect_batches(lf, chunk_size))


def validate_xlsx(path: Path, *, rows: int, sheets: int) -> bool:
    with zipfile.ZipFile(path) as zf:
        names = set(zf.namelist())
        for idx in range(1, sheets + 1):
            data = zf.read(f"xl/worksheets/sheet{idx}.xml")
            expected_rows = rows + 1
            got_rows = data.count(b"<row ")
            if got_rows != expected_rows:
                raise RuntimeError(
                    f"sheet{idx} rows: expected {expected_rows}, got {got_rows}"
                )
        return "xl/sharedStrings.xml" in names


def profile_xlsx_write(
    lf: pl.LazyFrame,
    *,
    rows: int,
    cols: int,
    sheets: int,
    chunk_size: int | None,
    autofit: AutofitMode,
    out_dir: Path,
) -> tuple[Path, int, bool]:
    out_dir.mkdir(parents=True, exist_ok=True)
    out = out_dir / f"xlsx_{rows}x{cols}x{sheets}_{autofit}.xlsx"
    if out.exists():
        out.unlink()

    with Workbook(out, chunk_size=chunk_size) as writer:
        for sheet_idx in range(sheets):
            writer.write_sheet(
                lf,
                f"S{sheet_idx + 1}",
                autofit=Autofit(mode=autofit),
            )

    has_shared_strings = validate_xlsx(out, rows=rows, sheets=sheets)
    return out, out.stat().st_size, has_shared_strings


def main() -> int:
    args = parse_args()
    if args.rows < 0:
        raise ValueError("--rows must be >= 0")
    if args.cols < 0:
        raise ValueError("--cols must be >= 0")
    if args.sheets < 1:
        raise ValueError("--sheets must be >= 1")
    if args.chunk_size is not None and args.chunk_size < 1:
        raise ValueError("--chunk-size must be >= 1 when provided")

    lf = make_lf(args.rows, args.cols)
    t0 = time.perf_counter()

    batches: int | None = None
    rows_seen = args.rows
    output_path: Path | None = None
    output_size_bytes: int | None = None
    has_shared_strings: bool | None = None

    if args.mode == "collect-only":
        batches, rows_seen = profile_collect_only(lf, chunk_size=args.chunk_size)
    elif args.mode == "arrow-drain":
        profile = profile_arrow_drain(lf, chunk_size=args.chunk_size)
        batches = profile.batches
        rows_seen = profile.rows
    else:
        output_path, output_size_bytes, has_shared_strings = profile_xlsx_write(
            lf,
            rows=args.rows,
            cols=args.cols,
            sheets=args.sheets,
            chunk_size=args.chunk_size,
            autofit=cast(AutofitMode, args.autofit),
            out_dir=args.out_dir,
        )

    elapsed = time.perf_counter() - t0
    result = ProfileResult(
        mode=args.mode,
        rows=rows_seen,
        cols=args.cols,
        sheets=args.sheets,
        cells=rows_seen * args.cols * (args.sheets if args.mode == "xlsx-write" else 1),
        chunk_size=args.chunk_size,
        elapsed_s=elapsed,
        peak_rss_kb=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
        batches=batches,
        output_path=str(output_path) if output_path is not None else None,
        output_size_bytes=output_size_bytes,
        output_size_mb=(
            output_size_bytes / 1024 / 1024 if output_size_bytes is not None else None
        ),
        has_shared_strings=has_shared_strings,
    )
    print(json.dumps(asdict(result), ensure_ascii=False), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
