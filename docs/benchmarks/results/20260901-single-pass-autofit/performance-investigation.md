# Single-Pass Autofit Performance Investigation

Date: 2026-09-01
Status: post-hoc diagnosis; not performance acceptance evidence

## Conclusion

The follow-up measurements do not reproduce the earlier huge-profile
`LazyFrame none` regression. Under interleaved baseline/candidate execution,
that control is effectively unchanged end to end (`-0.13%`) and is faster in
the writer stage (`-2.76%`). The earlier `+10.775%` result is therefore not
stable evidence of an implementation regression.

The single-pass implementation has a measurable benefit, but the benefit is
more specific than a general end-to-end speedup:

- for wide `body/all` workloads, writer CPU falls by about 5% and pre-close
  peak RSS falls by 26-28%;
- with full-row width tracking, writer CPU falls by 6.65% and pre-close peak
  RSS falls by 24.86%;
- with a more expensive LazyFrame source, writer CPU falls by 13.02% and
  pre-close peak RSS falls by 29.91%; and
- total wall-clock improvement is smaller and less stable because workbook
  close/ZIP compression is an independent, dominant cost.

The original comparison remains authoritative for the frozen acceptance gate,
and its status remains **performance acceptance not met**. This investigation
explains the observed behavior and improves the design of a future acceptance
run; it does not retroactively reclassify that result.

## Question

The investigation tested four explanations for the original result:

1. a real writer regression in `none/header` controls;
2. benchmark ordering and shared-host noise;
3. source evaluation being too cheap for removal of one evaluation to matter;
4. remaining cell formatting and ZIP work dominating the total.

## Compared artifacts and controls

The artifacts match the original comparison:

- baseline: commit `85c3519e7fd830c83dd0b3e52b00ff8db19c4ab3`,
  bridge ABI 5, package `0.2.0`;
- candidate: single-pass working tree, bridge ABI 6, package `0.2.0`;
- CPython 3.13.11, Polars 1.43.1, release native extensions;
- Linux 4.18, 192 logical Intel Xeon E7-8890 v4 CPUs; and
- XFS-backed `/tmp` fixtures and outputs.

Each comparison used a fresh process with `POLARS_MAX_THREADS=4` and
`taskset -c 0-7`. Baseline and candidate runs were interleaved in ABBA-style
blocks instead of running every baseline first and the candidate afterward.
Workbooks were checked after the timed write/close region. The investigation
records `write_sheet` and `close` wall times separately, process CPU for each
stage, output size, and the process RSS high-water mark immediately before
close.

The host was still shared. Its observed load averages varied during the run,
so the results are diagnostic rather than portable performance guarantees.
The sample counts are also too small to serve as a new acceptance baseline.

## Paired results

All changes below are candidate relative to baseline; negative time/CPU/RSS is
an improvement. Times, CPU, and RSS are medians. `Write RSS` is the high-water
mark captured at the end of `write_sheet`, before workbook close and before the
post-write XML validator.

| Workload | Runs per version | Total | Write | Close | Write CPU | Write RSS |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| tall 250k x 15, `none` | 4 | -0.13% | -2.76% | +0.45% | -2.69% | +1.31% |
| wide 50k x 61, `header` | 2 | -3.02% | +0.43% | -7.63% | +0.45% | -0.01% |
| wide 50k x 61, `body`, 20k sample | 4 | -2.87% | -3.52% | -3.78% | -5.31% | -26.08% |
| wide 50k x 61, `all`, 20k sample | 4 | -2.70% | -2.68% | -3.57% | -5.30% | -28.20% |
| wide 50k x 61, `all`, full scan | 2 | -0.50% | -3.14% | +5.05% | -6.65% | -24.86% |
| wide 50k x 61, `all`, full scan, computed source | 2 | -3.39% | -7.52% | +3.84% | -13.02% | -29.91% |

The corresponding medians make the scale clearer:

| Workload | Version | Total (s) | Write (s) | Close (s) | Write CPU (s) | Write RSS (MiB) |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| tall `none` | ABI 5 | 7.7244 | 4.2906 | 3.4338 | 4.3483 | 106.0 |
| tall `none` | ABI 6 | 7.7147 | 4.1720 | 3.4491 | 4.2315 | 107.4 |
| wide `body`, 20k | ABI 5 | 7.0481 | 4.4084 | 2.6397 | 4.4318 | 139.2 |
| wide `body`, 20k | ABI 6 | 6.8455 | 4.2533 | 2.5399 | 4.1966 | 102.9 |
| wide `all`, 20k | ABI 5 | 6.9842 | 4.3698 | 2.6031 | 4.4705 | 141.9 |
| wide `all`, 20k | ABI 6 | 6.7957 | 4.2527 | 2.5102 | 4.2334 | 101.8 |
| wide `all`, full | ABI 5 | 7.9668 | 5.4035 | 2.5633 | 5.5219 | 139.1 |
| wide `all`, full | ABI 6 | 7.9267 | 5.2339 | 2.6928 | 5.1550 | 104.6 |
| computed `all`, full | ABI 5 | 8.8176 | 5.6123 | 3.2053 | 6.7241 | 206.9 |
| computed `all`, full | ABI 6 | 8.5185 | 5.1902 | 3.3283 | 5.8485 | 145.0 |

The raw records are retained as [paired matrix](paired-matrix.json),
[additional body/all pairs](paired-extra.json),
[full-scan pairs](paired-fullscan.json), and
[computed-source pairs](paired-heavy.json).

## Why the overall speedup is modest

The 250,000 x 15 Parquet fixture took only about 0.08 seconds to collect or
drain through Arrow, while writing and closing its XLSX took 7-8 seconds. On
that workload, eliminating one input evaluation can improve the total by only
about one percent before considering any other costs. The original benchmark
therefore emphasized XLSX generation rather than source replay.

In the computed-source experiment, one evaluation alone took roughly
0.39-0.42 seconds wall time and 0.9-1.0 seconds process CPU. The single-pass
writer then showed a larger write-stage benefit. This supports the mechanism:
the gain grows when replaying the LazyFrame is expensive, while simple Parquet
scan/decompression is not the dominant cost on this host.

Removing the second source evaluation also removes its DataFrame intermediate.
That is consistent with the 25-30% reduction in writer-phase peak RSS for
`body/all`. `none/header` do not need body-width state and therefore do not show
the same memory change.

## CPU profile

One `perf record` sample per ABI was collected for the wide `body` case using
user-space cycles, a 199 Hz sampling frequency, and DWARF call graphs. This is
a hotspot probe rather than a quantitative paired benchmark. Approximate
sampled cycles were 20.22 billion for ABI 5 and 18.59 billion for ABI 6.

The dominant inclusive costs were common to both implementations:

| Area | Approximate share | Interpretation |
| --- | ---: | --- |
| zlib-rs match/deflate routines | 32% | ZIP compression dominates a large part of total work |
| format hashing (`SipHash`) | 13-15% | per-cell format lookup/deduplication remains expensive |
| autofit width formatting | 5.6% ABI 5; 7.2% ABI 6 | single pass still has to format and measure sampled values |
| constant-memory row flush | about 6.5% | writing worksheet XML remains material |
| ABI 5 width-planner batch scan | about 1.3% | removed work exists, but was not the largest hotspot |
| Parquet ZSTD decompression | below 1% | fixture source I/O is cheap relative to XLSX creation |

The percentages are not additive across every stack frame and should not be
used as a portable cost model. They are sufficient to show why removing the
planning pass does not make the workbook twice as fast: width calculation,
cell formatting, XML output, and ZIP compression still happen.

The retained textual reports are [ABI 5](perf-baseline-body.txt) and
[ABI 6](perf-candidate-body.txt). The original `perf.data` files are not
committed because they total about 65 MiB, embed host-specific mappings, and
are not portable across binaries.

## Evidence checksums

```text
79feb8ed5bd1a08a9fd1e51a73ae973f6458c213d5cb0a031eab254e70e8090c  paired-extra.json
b3ea10df9e5b480cd3a3fe91912967caf32de3c820102c00415921fbf72691cd  paired-fullscan.json
7fef7f41374c4c24d743830cd5c010473d5cf8bd3ec61f264b2ef0fe792cdfc0  paired-heavy.json
3f9ad32f8f04e7743b09451ceefe9386b80a170a0abb36a1293de89a9086dc70  paired-matrix.json
b13a330d838f8087b863de8b468a2a95f50408731765d924d297a7320dbeb315  perf-baseline-body.txt
7a607c5018c7df957c9c05d8a412cb71182a4cdb5c3d22cf1921fe38f7094790  perf-candidate-body.txt
```

## Candidate follow-up optimizations

These are separate changes and are not included in the single-pass branch:

1. **Native zlib feasibility.** `rust_xlsxwriter 0.90.2` documents an optional
   `zlib` feature for faster compression. neatxlsx currently enables only
   `constant_memory`. This targets the largest measured hotspot, but it changes
   the native dependency/build surface and needs Linux, macOS, Windows, and
   wheel-build validation before adoption.
2. **Width-tracker hot path.** Avoid tracking branches after `max_rows` is
   exhausted and avoid temporary numeric formatting allocations where the
   exact existing display-width semantics can be preserved. This is narrower
   and lower risk, but the likely total gain is modest.
3. **Format lookup.** Investigate precomputed per-column metadata and safe use
   of column formats to reduce per-cell hashing. This has more potential but is
   compatibility-sensitive because inferred formats, scientific notation,
   explicit overrides, missing values, and header/body precedence can require
   different cell formats. It should be designed and validated independently.

The order above follows measured cost and contract risk. No optimization should
be accepted from a microbenchmark alone; output values, formats, widths,
pagination, column splitting, and reader compatibility remain required oracles.

## Revised acceptance protocol

A future performance acceptance run should be predeclared and use:

- randomized or balanced interleaved baseline/candidate blocks;
- a dedicated quiet host, or fixed CPU affinity and fixed Polars thread count;
- at least 4-5 measured samples per version and scenario;
- separate `write_sheet` and `close` wall and process-CPU measurements;
- pre-close RSS separated from any XML validator allocation;
- both default `max_rows=20_000` and `max_rows=None` axes;
- both cheap Parquet scans and a specified computed LazyFrame source;
- output validation after the timed region; and
- exact source/native hashes, workload definitions, affinity, host load, and
  raw records.

The existing “baseline twice, then candidate” ordering is vulnerable to time
drift on a shared host. The huge profile also used only two samples, so its
medians and standard deviations were easily moved by close/compression
variance. Interleaving does not eliminate host noise, but it makes that noise
less likely to align with one implementation.
