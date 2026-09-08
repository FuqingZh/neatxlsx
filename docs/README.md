# neatxlsx Documentation

Start with the current contracts, then use the guides, plans, and historical
records for their stated scope:

Current delivery: [2026-09-08 release readiness](implementation-plan/20260908-v1.0-single-pass-release-readiness.md) records fresh checks and remaining merge/publication gates.

1. [Architecture](architecture/20260728-v1.0-neatxlsx-architecture.md) for
   component ownership, data flow, transactions, and large-table behavior.
2. [Public API contract](architecture/20260728-v1.0-neatxlsx-public-api-contract.md)
   for names, defaults, lifecycle, errors, and compatibility.
3. [Streaming and autofit architecture](architecture/20260831-v1.0-neatxlsx-streaming-autofit-architecture.md)
   for the canonical one-stream v6 path, logical sampling and split semantics,
   delayed-width finalization, constant-memory mechanics, and failure boundary.
4. [Single-pass body autofit implementation plan](implementation-plan/20260901-v1.0-single-pass-autofit-implementation-plan.md)
   for the confirmed delivery slices, compatibility finalization, modification
   inventory, tests, benchmarks, and rollback gates. It records completed
   functional evidence and the unmet performance gate; it is not the
   caller-facing behavior authority. Its accepted follow-on is the
   [display-accurate autofit optimization plan](implementation-plan/20260901-v1.0-display-accurate-autofit-optimization-plan.md),
   which composes `ssfmt`, `cell_autofit_width()`, the logical online tracker,
   CJK fallback evidence, `bestFit`, and narrow structural/visual/performance
   gates without adding a second public autofit mode.
5. [Interleaved benchmark and native zlib validation plan](implementation-plan/20260901-v1.0-interleaved-benchmark-zlib-validation-plan.md)
   for the fixed dual-environment comparison harness, paired verdict rules,
   compressor-identity stop gate, and wheel portability boundary. The completed
   [native zlib canary](benchmarks/results/20260901-native-zlib-canary/README.md)
   proved that C zlib was active but rejected adoption after five confident
   performance regressions.
6. [Caller migration guide](how-to-guides/20260831-v1.0-migrate-axiomkit-xlsx.md)
   for mapping axiomkit XLSX calls to the current API, lifecycle, and ZIP64
   reader compatibility.
7. [Future optimization plan](implementation-plan/20260810-v1.0-neatxlsx-future-optimization-implementation-plan.md)
   for the accepted product priorities, phased delivery, decision gates, and
   acceptance criteria. It records future direction, not current API behavior.
8. [M0/M1 report-trust design proposal](architecture/20260811-v1.0-neatxlsx-m0-m1-report-trust-design-proposal.md)
   for the recommended value, dtype, temporal, warning, format, and reference
   artifact semantics. M0 and the M1 implementation are landed; release
   publication remains a separate authorized step.
9. [M0/M1 report-trust implementation plan](implementation-plan/20260811-v1.0-neatxlsx-m0-m1-report-trust-implementation-plan.md)
   for the ordered delivery slices, affected components, validation, acceptance,
   and rollback gates. M0 and M1 implementation are complete locally; CI and
   release publication remain separate gates.
10. [Test plan](testing/20260728-v1.0-neatxlsx-test-plan.md) for test layers,
   oracles, CI lanes, and release evidence.
11. [Benchmark protocol](benchmarks/20260831-v1.0-xlsx-writer-benchmark-protocol.md)
   for current commands, workloads, output checks, memory-measurement limits,
   and requirements for new baselines. The retained ABI v5/v6 measurements and
   their non-acceptance conclusion are in the
   [single-pass comparison](benchmarks/results/20260901-single-pass-autofit/comparison.md)
   and its post-hoc
   [performance investigation](benchmarks/results/20260901-single-pass-autofit/performance-investigation.md).
   The clean attribution run, compact LibreOffice CJK canary, profiles, and
   failed display-autofit performance gate are retained in the
   [display-accurate autofit evidence](benchmarks/results/20260901-display-accurate-autofit/README.md).
   The later native-zlib decision is retained separately in the
   [canary record](benchmarks/results/20260901-native-zlib-canary/README.md).
12. [0.2.0 release notes](release-notes/20260811-v0.2.0-m1.md) for the M1 value
   semantics and additive row/header/body column-format controls included in
   the published release.
13. [Completed extraction migration](implementation-plan/20260728-v1.0-axiomkit-xlsx-extraction-migration-plan.md)
   for historical extraction checkpoints and the independence boundary.

## Historical source material

[axiomkit XLSX archive](archive/axiomkit-xlsx/README.md) contains the original
streaming plan, API/ZIP64 migration notes, and the full benchmark documentation
and result pairs omitted from the initial extraction. Its manifest pins source
commit, file sizes, and hashes; its coverage map links each topic to the current
neatxlsx document.

Archived API names, commands, and benchmark results are historical evidence,
not current contracts or performance claims. The extraction plan is completed
history; future plans do not establish deployment or publication. The August
2026 documentation migration did not change the published 0.2.0 release; the
current writer implementation and any later package publication remain
separate evidence boundaries.
