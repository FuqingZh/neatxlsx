# Native zlib canary: rejected

Date: 2026-09-01
Source commit: `54ef1e6279877c76c3b5e8fddadb1a509fd79d7e`
Host scope: local Linux x86_64
Decision: do not enable `rust_xlsxwriter/zlib` in production

## Decision

The experiment activated stock C zlib, but it was slower than the existing
`zlib-rs` backend in all five predeclared ZIP-heavy scenarios. Candidate/control
median total-wall ratios ranged from `1.1608` to `1.2977`; every 95% confidence
interval was wholly above `1.00`. Close-stage time was the dominant regression,
with median ratios from `1.3766` to `1.5649`.

The performance adoption gate therefore failed. The cross-platform wheel and
sdist matrix was not run because it can establish portability but cannot rescue
a failed performance prerequisite. No zlib feature or new C dependency is
retained in the production manifests. This local result is sufficient to reject
adoption for this source, dependency graph, and host; it is not a claim that C
zlib is slower on every platform or workload.

## Isolated builds

Both wheels were built from a `git archive` of the exact source commit with
separate Cargo target and wheel directories. The control used the committed
feature set. The experiment used temporary feature forwarding in the archived
source only:

```toml
# crates/neatxlsx_core/Cargo.toml
[features]
native-zlib-canary = ["rust_xlsxwriter/zlib"]

# crates/neatxlsx_py/Cargo.toml
[features]
native-zlib-canary = ["neatxlsx-core/native-zlib-canary"]
```

The direct dependency feature could not be selected from the workspace package
with Maturin, so this forwarding made the canary explicit without changing the
repository manifests. The control and experiment were built with:

```text
CARGO_TARGET_DIR=/tmp/neatxlsx-zlib-canary/target-control \
  .venv/bin/maturin build --release \
  --out /tmp/neatxlsx-zlib-canary/dist-control

CARGO_TARGET_DIR=/tmp/neatxlsx-zlib-canary/target-experiment \
  .venv/bin/maturin build --release \
  --features pyo3/extension-module,native-zlib-canary \
  --out /tmp/neatxlsx-zlib-canary/dist-experiment
```

| Artifact | Control SHA-256 | Experiment SHA-256 |
| --- | --- | --- |
| `Cargo.lock` | `8abfc60890850338f57d07c570a1eb6ab0aee8ab7a144fbf13c3c8a82abff15f` | `e5b6caa3a4531fe197da0fcdb2d570d124a54a27975629a768c68652d7d6cc28` |
| wheel | `90dade275cf00850747475f4d0d6b755818a5390c5a8cf3e33e48da0ec7dd9fb` | `f2172fae05ed558485baca9e02ec515ffa4beca0cb9bfcf5d6c1b6b981d940aa` |
| native extension | `7f97fa1597fc1f59c5792833197a04d5e02c6f447340bbced27a9b2a94cda33d` | `c6e019f1e499138bb574137096cfdd1d1098a896af48c928d82eae705aeb1c4b` |

[`cargo-lock.diff`](cargo-lock.diff) records the addition of `libz-sys 1.1.29`
and `vcpkg 0.2.15` to the experiment lock.

## Backend identity

The identity stop gate passed:

- [`control-feature-tree.txt`](control-feature-tree.txt) contains `zlib-rs`
  and no `libz-sys`.
- [`experiment-feature-tree.txt`](experiment-feature-tree.txt) contains both
  `libz-sys` and `zlib-rs` because Cargo features are additive.
- [`control-ldd.txt`](control-ldd.txt) has no `libz.so`; the
  [`experiment linkage`](experiment-ldd.txt) resolves `/lib64/libz.so.1`.
- The sampled experiment workload enters `deflate` through
  `flate2::ffi::c::Deflate::compress_inner`, as retained in
  [`experiment-perf-report.txt`](experiment-perf-report.txt).

The formal comparison was therefore measuring different active compression
backends, rather than two binaries with merely different resolved features.

## Interleaved measurement

The accepted comparison harness ran five scenarios with four deterministic
ABBA/BAAB blocks, one warmup per variant, and eight measured samples per
variant and scenario. Every sample ran in a fresh process with CPUs `0-7`,
`POLARS_MAX_THREADS=4`, order seed `20260901`, analysis seed `20260902`, and
10,000 paired bootstrap resamples. All 90 worker records completed.

| Scenario | Total wall ratio | 95% CI | Write wall ratio | Close wall ratio | Verdict |
| --- | ---: | ---: | ---: | ---: | --- |
| tall, `none` | 1.2189 | [1.1262, 1.2679] | 0.9885 | 1.4834 | rejected |
| tall, `header` | 1.2977 | [1.2041, 1.3592] | 1.0651 | 1.5649 | rejected |
| wide, `body` | 1.1676 | [1.1222, 1.2010] | 1.0254 | 1.3766 | rejected |
| wide, `all` | 1.1608 | [1.1345, 1.1954] | 1.0045 | 1.4081 | rejected |
| wide, `all`, full scan | 1.1657 | [1.1494, 1.1800] | 1.0288 | 1.4328 | rejected |

Ratios are candidate C zlib divided by control zlib-rs. Lower is faster. All
workbooks had identical uncompressed ZIP member names and normalized member
hashes. The only normalization was generated `docProps/core.xml`
`created`/`modified` UTC values; raw hashes are retained per run.

The frozen run order is in [`planned.json`](planned.json), measurements are in
[`raw.jsonl`](raw.jsonl), and the corrected verdict summary is in
[`comparison.json`](comparison.json) and [`comparison.md`](comparison.md).

## Verdict-label correction

The first analyzer version classified a confidence interval wholly above
`1.00` as `inconclusive/zlib_gate_unmet`. That label was too weak: the same raw
measurements establish a confident regression. The analyzer was corrected to
emit `rejected/zlib_regression` and the unchanged `raw.jsonl` was reanalyzed.

The original summaries remain as [`comparison-original.json`](comparison-original.json)
and [`comparison-original.md`](comparison-original.md). The corrected JSON
contains an `analysis_revision` object with the original summary hash, analyzer
hash, and `measurement_records_changed: false`. No measurements were rerun or
sample count changed after inspecting the result.

[`checksums.txt`](checksums.txt) covers the retained evidence files. Trailing
spaces were removed from copied text reports before hashing; measurement JSON
was retained byte-for-byte. Generated workbooks, installed wheels, Cargo
targets, fixtures, and the binary
`perf.data` remain outside the repository; their smaller textual or hashed
evidence is retained here.
