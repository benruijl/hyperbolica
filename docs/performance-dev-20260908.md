# Latest-dev update and performance, 2026-09-08

> Earlier benchmark jobs and report watchers were stopped by user request.
> A fresh complete-suite run is tracked in [the new live report](full-suite-20260908.md).
> Running/queued labels below belong to the preserved earlier snapshot.

The [parallel collection follow-up, September 9](parallel-collection-20260909.md)
implements concurrent coefficient collection and memory-aware lookahead, with
separate comparisons against the previous Rust build and HyperFLINT.

The [parallel scaling investigation, September 9](parallel-difference-20260909.md)
compares HyperFLINT's scheduling, collection and cache reuse with Rust, including
fresh matched controls and diagnostic queue-window experiments.

The [rational-handle multicore follow-up](multicore-overhead-20260908.md)
measures the latest ownership change against the pinned Rust build on identical
CPU sets. Those separate measurements do not replace the ongoing suite's samples.

<!-- CURRENT-BENCHMARK-STATS -->
The [latest Flint/Symbolica table](latest-backends-20260908.md) tracks the
scratch-fixed build and current qbox runs. The completed measurements below
retain their earlier build identities; the collaborator Rust queue now selects
the scratch-fixed build.

Memory-optimized build statistics are in the [optimized report](performance-optimized-20260908.md):
**162/162 corpus cases verified**, 486 fresh timed samples; **165 serial cases**
including the separately measured hard attachments. New serial `tst3` uses
**1249.89 MiB**; its eight-worker run uses
**1326.99 MiB** and takes **389.183 s**.
The new eight-worker `tst4` and both quadruple-box jobs and their provisional/final status are
tracked in that report. Earlier tables and snapshots below retain their
original executable, sample counts and measurement conditions; they are not
substitutes for the current summary.
<!-- END-CURRENT-BENCHMARK-STATS -->

## Earlier measurements and implementation record

The selected dependency is official Symbolica `dev` at
`fb845d34bda8ccf1fedef6544d3aa46dc24944e3`, fetched at the start of this run,
plus retained generic optimizations and the scalar-only correction described
below. HyperLica production Rust source is unchanged from `8dc4f6e`.
This report supersedes old results only where fresh measurements are given.

## Subsequent memory improvements

Smaller lazy-view storage and incremental finite-term collection are now
implemented and validated in the working tree. See the
[implementation measurements](memory-collection-20260908.md) for the new release
results. The new Rust `tst3` run completed in **1254.011 s** at **1249.89 MiB**
peak RSS, compared with the previous 1183.816 s and 20860.42 MiB. Its exact
output matches both archived backends. The source
identity and paired measurements below describe the earlier archived build.
The optimized eight-worker `tst3` follow-up took **389.183 s** at
**1326.99 MiB**, an observed **3.222×** speedup over
the optimized serial sample, with exact output agreement.
The [full attachment report](hf-benchmarks-20260908.md) tracks the continuing
original benchmark, including `tst4`.

## Source update and correctness

Upstream `bd29c47` supplies the previous constructor/context fixes and the
zero-operand univariate-GCD normalization that caused the Galois-upgrade panic.
Pristine latest dev passed all 604 native library tests and all 18 invocations
of the five recent standalone MRE examples, including 300 Galois attempts in
three fresh processes. The historical F4 defect is also covered by the passing
native `prevent_live_basis_update` regression; its archived standalone example
was not rerun separately. The root-interval expectation was explicitly aligned with
upstream's corrected `[3/16, 9/32]` initial interval; its refined interval is
unchanged. This was an expectation correction, not a changed mathematical root.

The retained broader exact-value matrix exposed an additional upstream corner
case: the scalar-only factored constructor for `1/2` could factor a constant
using an empty variable map and panic. Symbolica commit `6e20db0` handles
constant bases through existing scalar normalization. After that generic fix,
the combined native suite passes **637/637**, including 25 focused factored
tests; 17 standalone regression invocations pass, including 100 additional
Galois attempts and the new scalar reproducer.

The corrected vendored HyperLica build passes all **449 library tests**,
remaining all-target tests/smokes, and all **79 C++ differential cases in both
debug and release** (24 require the documented exact normalization).
Benchmark-evidence tests (47), matrix-harness tests (15), and
`cargo check --locked --all-targets --all-features` pass. Package-scoped
formatting passes; workspace-wide Rustfmt 1.89 reports an unchanged upstream
line-wrap difference in `src/transcendental.rs:4873`.

The active vendor retains concurrent cleanup of five now-upstream-fixed MRE
examples. Native tests remain, as does `examples/mre_factored_scalar.rs`; old
standalone examples are recoverable from the historical patch and archived
validation evidence. Cleanup does not change production library sources.
The exact dependency trees, cumulative patch and source/license notices are
recorded in [`../vendor/SYMBOLICA.md`](../vendor/SYMBOLICA.md).

## Measurement policy

Measurements use a fresh process per invocation, one warmup and three adjacent
alternating C++/Rust pairs, with exact preflight and output-repeatability checks.
Times include startup, parsing and serialization. The paired ratio is the
geometric mean of individual Rust/C++ ratios, not the ratio of the medians.
Every case is single-threaded and pinned to CPU145 (AMD EPYC9754, NUMA4,
singleton SMT sibling list). That core was idle in three selection samples;
the host is shared and affinity does not reserve it. Build/validation work is
assigned other cores. Small API timings include substantial process overhead.

The C++ executable/runtime adapter and FLINT3.6.0 library are unchanged and
hashed. Rust uses 1.89.0, release optimization, codegen-units=1 and LTO, without
a target-CPU override. Source/binary archives and exact per-case requests,
responses, bounds, times, RSS and paired ranges are retained. A failed,
unselected or preprocessing-dependent case is never counted as runtime parity.

## Final scalar-corrected release: complete 162-case comparison

These fresh results use the corrected executable SHA-256
`cf72945dcada1632e31dca72e883ad91997d741e755b8050af335a16963ffea4`,
with Symbolica production source at `6e20db0`. Final vendor tip `15bd1f6`
has identical production sources; its remaining changes remove obsolete
examples and update their index. Full selected source tree:
`7b1c3dde94a48d77bf072f58a54a610e9964240d`.

All 162 selected cases passed exact comparison and repeatability checks. All
162 paired runtime ratios, and all 486 individual timed pair ratios, favor
HyperLica in this run. This is exploratory evidence on the selected corpus,
not a universal or memory-parity qualification.

| Group | Cases | Geometric mean Rust/C++ ratio |
|---|---:|---:|
| Locked core workloads | 14 | 0.388 |
| Differential/API workloads | 79 | 0.422 |
| Generated scaling workloads | 65 | 0.418 |
| Light attachment cases | 4 | 0.244 |
| All selected cases | 162 | 0.412 |

The overall paired geometric mean corresponds to **2.43× faster** on this
selected, equally weighted corpus. It is not a claim that upstream's update
alone produced that gain: the candidate includes retained local optimizations,
and this run compares HyperLica with HyperFLINT, not old Symbolica with new.

| Case | HyperFLINT ms | HyperLica ms | Paired Rust/C++ ratio | Peak MiB C++ / Rust |
|---|---:|---:|---:|---:|
| `tst0` | 453.138 | 137.028 | 0.302 | 18.06 / 22.54 |
| `tst1` | 7,974.962 | 3,692.283 | 0.464 | 35.23 / 69.87 |
| `findroots21_a` | 118.321 | 13.522 | 0.115 | 27.22 / 14.47 |
| `findroots21_b` | 41.519 | 9.037 | 0.218 | 27.18 / 14.49 |
| Dense-parameter resultant | 1,980.700 | 310.633 | 0.157 | 43.61 / 67.51 |
| Dense power, degree12 / 3 variables | 19.185 | 8.623 | 0.435 | 15.61 / 12.04 |
| Four sixth-order poles | 49.366 | 28.829 | 0.584 | 23.09 / 26.57 |
| Zero-to-one period, weight4 | 25.445 | 8.070 | 0.333 | 15.05 / 12.08 |
| Empty algebraic-letter control | 14.277 | 5.362 | 0.380 | 12.08 / 12.08 |

The three `tst1` paired ratios span 0.462984–0.464926, corresponding to a
2.16× paired speedup. Four sixth-order poles span 0.574930–0.596788.
Three pairs are exploratory evidence, not a confidence interval or formal
qualification run. Tiny tasks are particularly sensitive to process overhead.

Rust peak RSS is higher in 55/162 cases; seven exceed C++ by at least 10%.
`tst1` is approximately 2×, the dense-parameter resultant 1.55×, the E26
order-verification regression 1.54×, `tst0` 1.25×, three sixth-order poles 1.21×,
the shared-denominator rational sum 1.20×, and four sixth-order poles 1.15×.
Runtime gains do not establish memory parity.

Six heavy inputs are unselected in this matrix, and 23 attachment inputs still
require SubTropica preprocessing. Historical heavy timeouts/allocation failures
are not fresh successful timings. See the earlier report for their old bounded
outcomes; expanding raw epsilon-dependent inputs is not the complete pipeline.

## Additional heavy-input probe

`parity_face` now completes successfully with **exactly equal output**. The
separate probe used the same core and executable, one timed pair after exact
preflight, no warmup, a 600-second per-process timeout and an 8GiB per-process
address-space limit. Both the preflight and timed outputs match and repeat.

| Case | HyperFLINT seconds | HyperLica seconds | Rust/C++ ratio | Peak MiB C++ / Rust |
|---|---:|---:|---:|---:|
| `parity_face`, one timed pair | 25.046 | 17.008 | 0.679 | 59.10 / 171.80 |

This is a 1.47× observed runtime advantage with 2.91× the memory, not a
multi-pair timing qualification. It supersedes the historical failed/rejected
`parity_face` outcome for this particular input. The other five heavy inputs
were not rerun. None of these results establishes the full 23-input SubTropica
epsilon/preprocessing pipeline. This probe is not included in the 162-case
aggregate above.

## Preserved preliminary run

`pre-scalar-fix-vs-cpp` measured the earlier `7094dda` build, executable SHA-256
`fa0ee1270ceed18e6d4304f35e9bbea8f8fa9a52ce125fc004174bbc5161a378`.
It passed 162 exact cases with an overall paired Rust/C++ ratio of 0.404;
`tst1` was 8,024.665ms C++ versus 3,714.839ms Rust. These are explicitly
pre-fix results, preserved rather than relabeled as the final release. The
two separate runs do not isolate the performance effect of the scalar fix.

## Reproducible evidence

Local workstation evidence is under `target/performance-dev-20260908/`:

- `PROVENANCE.md` distinguishes source snapshots and executable identities.
- `final-vs-cpp/{results.json,summary.csv,overview.md}` contains the corrected
  release's complete 191-row inventory and all 162 measured cases.
- `final-parity-face/` retains the separate heavy-input probe.
- `pre-scalar-fix-vs-cpp/{results.json,summary.csv,overview.md}` contains the
  first complete 191-row inventory and all 162 measured cases.
- `native-patched/scalar-fixed/README.md` records the additional scalar failure,
  forced local-crate rebuild, corrected native tests and saved binary hashes.
- `../latest-dev-pristine-mres-20260908/README.md` records pristine native/MRE
  validation, matched dependency compilation and archived source/binaries.
- `cpu-selection.txt` records topology and the short utilization samples.

These ignored raw artifacts are not included in a fresh clone. Checked-in
fixtures, scripts, dependency patches and source identities permit new runs.

## Full attachment follow-up

The subsequent [all-attachment report](hf-benchmarks-20260908.md) records
fresh attempts of all ten direct `hf_benchmarks.json` inputs with this same
scalar-corrected executable, including the five heavy cases omitted above.
It retains all 33 attachment rows and distinguishes completed comparisons,
bounded failures, and the 23 cases still requiring SubTropica preparation.
Its progress/completion status and exact run conditions are recorded separately;
the 162-case measurements above remain historical evidence for their own run.
