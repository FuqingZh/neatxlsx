# 2026-09-08 release readiness: blocked

The fresh display-autofit comparison stopped after the first mandatory control
completed all eight predeclared pairs and failed. The candidate production
source is unchanged from the 2026-09-01 estimator candidate; its native binary
was rebuilt locally for this run. Original thresholds and sample counts remain.

`none` total-wall median ratio is 1.019782, with paired-bootstrap 95% interval
[1.002116, 1.045764]. The required upper bound is below 1.03. All eight output
pairs have equal normalized ZIP members. This is not a 4.58% measured median
regression: 4.58% is the uncertainty upper bound.

The matrix is **incomplete**, and acceptance is **false**. There are 20 completed
records: 18 for `none` (two warmups, 16 measured runs) and two `header` warmups.
One later worker was terminated when remaining work was stopped. No measured
`header`, `body`, `all`, or full-scan result is claimed from this run. The driver
and its observed child were stopped and checked for live processes.

The current analyzer generated `comparison.json` using all retained records,
with `termination_reason` and `matrix_complete=false`. Its finalizer returned 1.
The historical six-scenario result remains independently failed (one of six
passed). This candidate-to-single-pass comparison does not establish the
separate two-pass-to-single-pass adoption gate.

Local full check and explicit pagination checks passed; logs are retained.
`candidate-source-manifest.json` pins source bytes copied before this delivery's
documentation commit; the copied source, built native library, and large test
outputs remain in task scratch space. It is an identification record, not a
self-contained distribution. Binary identity and measured environment are in
`raw.jsonl`; build/check logs describe the separate local validation.

Merge and release remain blocked by performance acceptance and the missing
Windows desktop Excel visual/edit checks. CI and wheel portability are separate
obligations. No benchmark threshold, public mode, fallback, or output contract
was changed to obtain acceptance.
