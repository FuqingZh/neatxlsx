# Single-Pass Autofit Benchmark Comparison

Date: 2026-09-01
Status: recorded; functional change validated, performance acceptance not met

## Compared artifacts

- Baseline writer: commit
  `85c3519e7fd830c83dd0b3e52b00ff8db19c4ab3`, package `0.2.0`, bridge ABI
  5 / `neatxlsx.xlsx.writer.v5`, release native SHA-256
  `a2d2426408178c1389f330018b0be76c58c3bde1cecca2d505eeced7a8375ecb`.
- Candidate writer: `codex/single-pass-autofit` working tree based on the same
  commit, package `0.2.0`, bridge ABI 6 /
  `neatxlsx.xlsx.writer.v6`, Arrow C Data transport, release native SHA-256
  `cdfa17f497d944c3459a0b93bb1f87cc49043975a537ac416e04dd13bd85adb8`.
- Both sides used CPython 3.13.11, Polars 1.43.1, the same benchmark harness
  SHA-256 `f946f296d85e387b2ef60d0407d850276b9146a941315a4ab176794e68adb3b3`,
  8,192-row batches, release builds, and XFS-backed `/tmp` output on the same
  host.
- The host has 192 logical Intel Xeon E7-8890 v4 CPUs. No benchmark process was
  intentionally run concurrently during the retained serial huge sequence;
  other shared-host load was not instrumented.

The baseline package and native binary came from an isolated archive of the
pinned commit. The candidate was a reviewable working tree rather than a source
commit at measurement time, so the native binary hash is the exact candidate
identity for these results. The complete baseline preparation is recorded in
[baseline-provenance.md](baseline-provenance.md).

## Frozen comparison rule

For each scenario, let `m1` and `m2` be the two baseline medians. The reference
is their midpoint. The noise allowance is the maximum of:

1. `abs(m1 - m2) / midpoint`; and
2. `stdev / mean` from either baseline run.

Candidate change is `(candidate_median - midpoint) / midpoint`; a negative
value is faster. A change is classified only when its magnitude exceeds the
frozen allowance. This rule was derived from the two baseline runs before the
retained candidate run. It classifies these measurements and is not a portable
performance guarantee.

## Default profile

| Scenario | Baseline 1 (s) | Baseline 2 (s) | Midpoint (s) | Candidate (s) | Allowance | Change | Classification |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| DataFrame header | 0.854432 | 0.911003 | 0.882717 | 0.843624 | 7.925% | -4.429% | within noise |
| DataFrame all | 1.083303 | 0.844002 | 0.963652 | 0.712832 | 24.833% | -26.028% | improvement |
| LazyFrame none | 0.895971 | 0.918435 | 0.907203 | 0.827130 | 4.976% | -8.826% | improvement |
| LazyFrame header | 0.897372 | 0.930809 | 0.914091 | 0.835578 | 23.539% | -8.589% | within noise |
| LazyFrame body | 0.794132 | 0.828598 | 0.811365 | 0.783383 | 8.856% | -3.449% | within noise |
| LazyFrame all | 0.867352 | 0.904985 | 0.886169 | 0.883789 | 16.771% | -0.268% | within noise |

Raw records: [baseline run 1](baseline-run1.json),
[baseline run 2](baseline-run2.json), and [candidate](candidate.json).

The default profile shows no regression outside measured noise. It does not
show the required LazyFrame `body` or `all` improvement outside noise.

## Huge profile, strictly serial sequence

The retained sequence ran baseline 1, baseline 2, and then the candidate, with
each process exiting before the next began.

| Scenario | Baseline 1 (s) | Baseline 2 (s) | Midpoint (s) | Candidate (s) | Allowance | Change | Classification |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| DataFrame header | 6.577599 | 6.693146 | 6.635372 | 6.790753 | 2.112% | +2.342% | regression |
| DataFrame all | 6.313500 | 6.477894 | 6.395697 | 6.118105 | 3.515% | -4.340% | improvement |
| LazyFrame none | 6.206483 | 6.247035 | 6.226759 | 6.897704 | 4.823% | +10.775% | regression |
| LazyFrame header | 6.967936 | 6.352760 | 6.660348 | 6.524016 | 9.236% | -2.047% | within noise |
| LazyFrame body | 6.223136 | 6.427123 | 6.325129 | 5.936453 | 3.225% | -6.145% | improvement |
| LazyFrame all | 6.163127 | 6.533342 | 6.348235 | 6.286643 | 5.832% | -0.970% | within noise |

Raw records: [baseline run 1](huge-baseline-run1.json),
[baseline run 2](huge-baseline-run2.json), and
[candidate](huge-candidate.json).

All scenarios passed the benchmark's independent dimension and row-count
validator. Default-profile output sizes were identical. Huge-profile candidate
archives differed from the baseline by zero or one compressed byte; archive
size is not used as a semantic oracle. Frozen reference manifests and real
split tests provide the separate behavior evidence.

## Acceptance result

The benchmark proves neither a general speedup nor the plan's full performance
gate. The huge LazyFrame `body` result improves beyond its allowance, while
LazyFrame `all` remains within noise. The huge LazyFrame `none` control regresses
beyond its allowance, and the DataFrame header control is narrowly outside its
allowance. The default profile has no regression but does not establish the
required `body/all` gains.

Therefore the single-stream routing is accepted only on functional and
structural evidence. Performance acceptance remains unmet, and this branch
must not be promoted on a performance claim without a separately designed
follow-up investigation.

The subsequent [performance investigation](performance-investigation.md)
separates writer and close costs, interleaves ABI 5/6 runs, measures pre-close
RSS, and records CPU hotspots. It explains the unstable controls and finds
writer-stage CPU and memory improvements, but does not retroactively change
this frozen acceptance result.

## Excluded measurements

The files prefixed `excluded-overlap-huge-` are retained for auditability but
excluded from every calculation above. Their baseline and candidate processes
overlapped on the same host, so resource contention made them non-comparable.
