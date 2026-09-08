# axiomkit XLSX source archive

Date: 2026-08-31
Status: historical source material; not the neatxlsx API or execution contract

## Provenance and scope

The 27 files listed in [manifest.json](manifest.json) were copied byte-for-byte
from axiomkit commit `d111dfa23e40e0a18a956bb2c5c3ee28641239d1`. Each manifest
entry records the original repository-relative path, byte count, and SHA256.
The source is the pinned
[FuqingZh/axiomkit revision](https://github.com/FuqingZh/axiomkit/tree/d111dfa23e40e0a18a956bb2c5c3ee28641239d1).

Verify the imported files without an axiomkit checkout, from the neatxlsx root:

```bash
python - <<'PY'
import hashlib
import json
from pathlib import Path

root = Path("docs/archive/axiomkit-xlsx")
manifest = json.loads((root / "manifest.json").read_text())
for entry in manifest["files"]:
    data = (root / entry["path"]).read_bytes()
    assert len(data) == entry["bytes"], entry["path"]
    assert hashlib.sha256(data).hexdigest() == entry["sha256"], entry["path"]
print(f"Verified {len(manifest['files'])} imported files")
PY
```

This archive preserves all tracked files in `docs/migration/xlsx*.md` and
`benchmarks/xlsx_writer/` at that revision: three migration/streaming documents,
a benchmark README and index, ten JSON/Markdown result pairs, and two comparison
documents. axiomkit remains unchanged and independent. The archive is a
snapshot, not a synchronized copy.

The broader axiomkit naming audit and data-format specifications are not XLSX
writer documentation and were not imported. The writer and measurement scripts
were already extracted into neatxlsx; they are referenced by the current guides
rather than copied again.

## Source-to-current-document map

| Archived source | Current neatxlsx destination | Treatment |
| --- | --- | --- |
| [Single-pass plan](docs/migration/xlsx_streaming_single_pass_plan.md) | [Execution and autofit architecture](../../architecture/20260831-v1.0-neatxlsx-streaming-autofit-architecture.md) | Preserve the original scope; explain actual code and the delayed-width alternative separately |
| [Writer v2 migration](docs/migration/xlsx_writer_v2.md) | [Caller migration guide](../../how-to-guides/20260831-v1.0-migrate-axiomkit-xlsx.md) | Retain old renames as history; rewrite examples and mappings for the current public API |
| [ZIP64 default](docs/migration/xlsx_zip64_default.md) | [Caller migration guide](../../how-to-guides/20260831-v1.0-migrate-axiomkit-xlsx.md#zip64-and-reader-compatibility) and [architecture](../../architecture/20260728-v1.0-neatxlsx-architecture.md#zip64-and-limits) | Keep the default and opt-out rationale; replace obsolete option names |
| [Benchmark README](benchmarks/xlsx_writer/README.md) | [Current benchmark protocol](../../benchmarks/20260831-v1.0-xlsx-writer-benchmark-protocol.md) | Rewrite commands, workload definitions, and measurement limits against the current scripts |
| [Benchmark result index](benchmarks/xlsx_writer/results/INDEX.md), result pairs and comparisons | This archive | Preserve historical measurements; do not promote them to a neatxlsx baseline |

## Historical execution decisions

- `e1623d825bfe643403b488b52592328b6f73152b` (2026-04-08) added the
  axiomkit v2 migration note. `8f861cfdaecdfe0eaa2de9a2fa5b699ffdc551ee` and
  `211cf4545956de22131b606882c10f3c0cb855b2` (2026-04-09) recorded scientific
  notation being disabled by default and removal of its old sampling option.
- `7735ae63ccf2e3a2a3aa60b4093bc0603f6db981` (2026-06-22), “Stream XLSX writes
  from Arrow batches”, introduced the archived single-pass plan. It explicitly
  scoped single-pass writing to `header/none` and retained `body/all` on the
  two-pass planner. It does not document a comparison with delayed width
  assignment, or establish that the XLSX format requires a pre-scan.
- `feea007bdbc78604d769a4c2a8d26be04b0a4fd0` (2026-07-28) enabled ZIP64 by
  default.
- neatxlsx's first commit, `bd0bed8c0791f28cddacfa73db3260c3b27dec8a`, copied
  these implementation paths under a behavior-equivalent extraction scope.
  Its direct DataFrame path already set widths after writing cells.

The documented optimization scope explains what was retained. It does not
establish the original author's unstated reason for excluding delayed widths.

## Reading historical benchmarks safely

The original index omits two preserved pairs:
[09:28:01 Markdown](benchmarks/xlsx_writer/results/xlsx_writer_20260212T092801Z.md)
and [JSON](benchmarks/xlsx_writer/results/xlsx_writer_20260212T092801Z.json), and
[09:28:44 Markdown](benchmarks/xlsx_writer/results/xlsx_writer_20260212T092844Z.md)
and [JSON](benchmarks/xlsx_writer/results/xlsx_writer_20260212T092844Z.json).
The manifest is the complete archive inventory; the original index is unchanged.
The [08:50 comparison](benchmarks/xlsx_writer/results/xlsx_writer_compare_20260212T0850Z.md)
and [10:08 comparison](benchmarks/xlsx_writer/results/xlsx_writer_compare_20260212T1008Z.md)
also retain their paired source results locally.

These measurements predate the June streaming change and the neatxlsx
extraction. They cannot prove the performance of either the current writer or a
future single-pass `body/all` implementation. Old commands, absolute binary
paths, package names, and backend options describe the recorded environment;
they are not runnable instructions for this checkout. Earlier records have less
validation metadata than the final pair. Do not infer missing commit IDs,
release-build evidence, peak RSS, or output checks from neighboring records.

The copied bytes are the source repository's August snapshot of the February
records. They are not claimed to be untouched original February artifacts:
axiomkit history includes later benchmark-path updates. No historical result
was rerun or revalidated by this documentation migration.
