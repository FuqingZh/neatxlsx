# neatxlsx Documentation

Read these current documents in order:

1. [Architecture](architecture/20260728-v1.0-neatxlsx-architecture.md) for
   component ownership, data flow, transactions, and large-table behavior.
2. [Public API contract](architecture/20260728-v1.0-neatxlsx-public-api-contract.md)
   for names, defaults, lifecycle, errors, and compatibility.
3. [Future optimization plan](implementation-plan/20260810-v1.0-neatxlsx-future-optimization-implementation-plan.md)
   for the accepted product priorities, phased delivery, decision gates, and
   acceptance criteria. It records future direction, not current API behavior.
4. [M0/M1 report-trust design proposal](architecture/20260811-v1.0-neatxlsx-m0-m1-report-trust-design-proposal.md)
   for the recommended value, dtype, temporal, warning, format, and reference
   artifact semantics. M0 and the M1 implementation are landed; release
   publication remains a separate authorized step.
5. [M0/M1 report-trust implementation plan](implementation-plan/20260811-v1.0-neatxlsx-m0-m1-report-trust-implementation-plan.md)
   for the ordered delivery slices, affected components, validation, acceptance,
   and rollback gates. M0 and M1 implementation are complete locally; CI and
   release publication remain separate gates.
6. [Test plan](testing/20260728-v1.0-neatxlsx-test-plan.md) for test layers,
   oracles, CI lanes, and release evidence.
7. [0.2.0 release notes](release-notes/20260811-v0.2.0-m1.md) for the M1 value
   semantics and additive row/header/body column-format controls included in
   this minor release.
8. [Completed extraction migration](implementation-plan/20260728-v1.0-axiomkit-xlsx-extraction-migration-plan.md)
   for historical extraction checkpoints and the independence boundary.

No current documents are archived or superseded for 0.2.0. The extraction
migration plan is retained as completed historical evidence.
