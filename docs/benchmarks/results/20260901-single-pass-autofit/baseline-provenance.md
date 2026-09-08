# neatxlsx ABI v5 baseline benchmark evidence

## Immutable writer/package source

- Source archive: `git archive 85c3519` from `/home/fqzhang/project/neatxlsx`.
- Resolved commit: `85c3519e7fd830c83dd0b3e52b00ff8db19c4ab3` (`0.2.0`, `origin/main`).
- No repository or worktree file was modified for this evidence run.

## Benchmark harness exception

The archive's original benchmark script did not contain the Parquet LazyFrame
scenarios required for the candidate comparison.  Only
`scripts/benchmark_xlsx_writer.py` was copied from
`/home/fqzhang/project/neatxlsx/.worktrees/single-pass-autofit` into this
otherwise source-commit archive.  Its SHA-256 is
`f946f296d85e387b2ef60d0407d850276b9146a941315a4ab176794e68adb3b3`.
The package and native writer remain v5 source at the commit above.

## Environment identity

- Project Python: `/tmp/neatxlsx-v5-benchmark/archive/.venv/bin/python` (CPython 3.13.11)
- `neatxlsx.__file__`: `/tmp/neatxlsx-v5-benchmark/archive/python/neatxlsx/__init__.py`
- `neatxlsx._native.__file__`: `/tmp/neatxlsx-v5-benchmark/archive/python/neatxlsx/_native.abi3.so`
- Cargo profile: `release`
- Bridge ABI / contract: `5` / `neatxlsx.xlsx.writer.v5`
- Package version / Polars: `0.2.0` / `1.43.1`

## Exact preparation commands

```sh
mkdir -p /tmp/neatxlsx-v5-benchmark/archive
git archive 85c3519 | tar -x -C /tmp/neatxlsx-v5-benchmark/archive
cp /home/fqzhang/project/neatxlsx/.worktrees/single-pass-autofit/scripts/benchmark_xlsx_writer.py scripts/benchmark_xlsx_writer.py
UV_CACHE_DIR=/tmp/neatxlsx-v5-benchmark/uv-cache PDM_CACHE_DIR=/tmp/neatxlsx-v5-benchmark/pdm-cache XDG_STATE_HOME=/tmp/neatxlsx-v5-benchmark/xdg-state pdm sync -G dev
UV_CACHE_DIR=/tmp/neatxlsx-v5-benchmark/uv-cache PDM_CACHE_DIR=/tmp/neatxlsx-v5-benchmark/pdm-cache XDG_STATE_HOME=/tmp/neatxlsx-v5-benchmark/xdg-state pdm run maturin develop --release --uv
```

The first `pdm run ... --help` attempt failed because uv tried to create a
temporary file under read-only `/home/fqzhang/.cache/uv`.  The first `pdm sync`
then failed writing a PDM log under read-only `/home/fqzhang/.local/state/pdm`.
Setting `UV_CACHE_DIR`, `PDM_CACHE_DIR`, and `XDG_STATE_HOME` as shown above
resolved those sandbox-only cache locations.  A stale zero-byte uv lock left by
the timed-out sync attempt was removed after confirming no PDM/uv process was
running.  The completed sync warned that locked Polars 1.43.1 is yanked; it
still installed the lockfile's exact version.

## Exact benchmark commands and raw outputs

```sh
UV_CACHE_DIR=/tmp/neatxlsx-v5-benchmark/uv-cache PDM_CACHE_DIR=/tmp/neatxlsx-v5-benchmark/pdm-cache XDG_STATE_HOME=/tmp/neatxlsx-v5-benchmark/xdg-state pdm run python scripts/benchmark_xlsx_writer.py --profile default --repeat 3 --warmup 1 --out-dir /tmp/neatxlsx-v5-benchmark/run1 2>&1 | tee /tmp/neatxlsx-v5-benchmark/run1/command-output.txt
UV_CACHE_DIR=/tmp/neatxlsx-v5-benchmark/uv-cache PDM_CACHE_DIR=/tmp/neatxlsx-v5-benchmark/pdm-cache XDG_STATE_HOME=/tmp/neatxlsx-v5-benchmark/xdg-state pdm run python scripts/benchmark_xlsx_writer.py --profile default --repeat 3 --warmup 1 --out-dir /tmp/neatxlsx-v5-benchmark/run2 2>&1 | tee /tmp/neatxlsx-v5-benchmark/run2/command-output.txt
```

- Run 1: `run1/xlsx_writer_20260901T044253Z.json` and `.md`, with raw terminal output in `run1/command-output.txt`.
- Run 2: `run2/xlsx_writer_20260901T044328Z.json` and `.md`, with raw terminal output in `run2/command-output.txt`.

Each JSON contains all raw timings, median, sample standard deviation, output
size, scenario definition, platform, Python, package versions, release binary
path, and timing policy.

## Retained serial huge-profile runs

The same verified archive, release native extension, harness, and cache
settings were used for both runs:

```sh
UV_CACHE_DIR=/tmp/neatxlsx-v5-benchmark/uv-cache PDM_CACHE_DIR=/tmp/neatxlsx-v5-benchmark/pdm-cache XDG_STATE_HOME=/tmp/neatxlsx-v5-benchmark/xdg-state pdm run python scripts/benchmark_xlsx_writer.py --profile huge --repeat 2 --warmup 1 --out-dir /tmp/neatxlsx-v5-benchmark/huge-serial-run1
UV_CACHE_DIR=/tmp/neatxlsx-v5-benchmark/uv-cache PDM_CACHE_DIR=/tmp/neatxlsx-v5-benchmark/pdm-cache XDG_STATE_HOME=/tmp/neatxlsx-v5-benchmark/xdg-state pdm run python scripts/benchmark_xlsx_writer.py --profile huge --repeat 2 --warmup 1 --out-dir /tmp/neatxlsx-v5-benchmark/huge-serial-run2
```

Run 1 completed before run 2 started. The retained records are
`huge-baseline-run1.{json,md}` (`2026-09-01T04:55:23Z`) and
`huge-baseline-run2.{json,md}` (`2026-09-01T04:57:44Z`). With two samples, the
reported median is their arithmetic mean and `stdev_seconds` is the sample
standard deviation. The JSON files are authoritative.

An earlier pair of huge baselines and an earlier candidate overlapped on the
same host. They are retained under the `excluded-overlap-huge-` prefix, are not
part of the frozen noise calculation, and support no comparison claim.
