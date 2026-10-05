# Causes of high memory use and mitigation experiments, 2026-09-08

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

Updated 2026-09-08T13:30:03.258721+00:00. The full attachment
benchmark remains independently managed by its original controller and watcher.
See [the ongoing benchmark report](hf-benchmarks-20260908.md) and
[the completed-case peak analysis](hf-memory-20260908.md).

The combined isolated prototype reduced `tst2` peak RSS from **1,093.09 to
105.15 MiB (90.38%, or 10.40× less)** while preserving the exact output. The
smaller lazy-view holder alone used 948.36 MiB. The combined `tst1` result was
70.84 → 45.62 MiB; `tst0` remained at its approximately 22.5 MiB startup footprint.
Exact-output checks passed on all four tested fixtures, including `parity_face`.
These prototypes have not been applied to the running benchmark or production code.

![Measured phases and mitigation results](../target/hf-memory-cause-20260908/memory-causes.png)

## Main finding

The dominant measured cause on the `tst1`/`tst2` integration path is accumulation
of uncollected endpoint terms. Harder inputs produce many more such terms,
and the driver retains them all until the end of each variable's integration.
The rational, coefficient, and word objects attached to those terms amplify the cost.

For `tst2`, integration over `t2` generates **687,419 raw finite terms**, which
collect to **1,164 terms**. Immediately after collection, dropping the raw list
releases **782.42 MiB** of requested live Rust allocations. The whole process's
peak requested Rust allocation payload is **887.44 MiB**, versus approximately
**1,093 MiB peak RSS**. The payload counter excludes native GMP allocations,
allocator metadata, resident unused pages, libraries, and stack.
The raw-list release is a lifetime measurement, not an additive decomposition
of peak RSS; shared references and temporary collection storage affect it.

The instrumented binary returns exactly the archived binary's output, excluding
only its timing field. Its `tst2` peak RSS is within 0.2% of the uninstrumented
control, supporting the relevance of these phase measurements.

| Integration variable | Input entries | Raw finite terms | Collected terms | Live Rust MiB freed by dropping raw finite list |
|---|---:|---:|---:|---:|
| `t4` | 55 | 543 | 331 | 4.60 |
| `t5` | 331 | 4,843 | 559 | 18.80 |
| `t1` | 559 | 97,051 | 4,354 | 160.17 |
| `t2` | 4,354 | 687,419 | 1,164 | 782.42 |
| `t3` | 1,164 | 56,628 | 51 | 50.41 |

The algorithm currently builds `Vec<EntryContribution>` for the whole step,
then extends another `finite` vector, and finally collects equal keys through
a borrowed reference to that vector. This keeps the raw data live throughout
collection. Vector growth during merging also temporarily overlaps old and new
storage. Relevant production sources: [driver](../src/integrator/integration_step/driver.rs),
[endpoint processing](../src/integrator/integration_step/entry.rs), and
[collection](../src/integrator/transform/collection.rs).

## Allocation-site evidence

A separate `tst1` heaptrack run with the diagnostic System allocator captured
**39.61 MiB of heap payload at the global peak**.
The output exactly matched the archive. Summing stacks with `process_entry`
accounts for **78.86%** of this live heap. These are allocations still live
at the global peak, not cumulative bytes allocated over the entire run.

The following buckets classify each allocation by its nearest HyperLica source
frame and are mutually exclusive:

| Nearest allocation-related source frame | Share of heap at peak |
|---|---:|
| `Rat::from_native` | 35.38% |
| `Rat::new` | 16.79% |
| `combine_keys` | 11.97% |
| `SymCoef::try_mul` | 6.93% |
| `Rat::try_mul` | 6.85% |
| `merge_contributions` | 4.45% |

These identify rational construction, word/key storage and symbolic coefficients
retained by endpoint contributions. They are not all polynomial-division scratch
space. Allocations under materialization of `Rat::compatibility_views` account
for just **0.02%** of heap at this peak, although the empty view holders themselves
are allocated by `from_native` for every new rational.

On this build, `Rat` itself is 24 bytes and clones already share its native
rational value and lazy views. However, each new rational allocates a
**152-byte** `Arc<OnceLock<CompatibilityViews>>`, including Arc counters,
regardless of whether its two polynomial views are ever initialized. The
native rational's separate Arc allocation requests another 128 bytes, before
its polynomial buffers. Boxing the compatibility payload reduces the empty
holder to **32 bytes** and allocates the full payload only when used.
See [rational representation](../src/core/rat.rs) and
[construction and compatibility views](../src/core/rat/representation.rs).

## Allocator controls

These runs use the original, unmodified production Rust executable on CPU194.
All return exactly the default result. They are separate diagnostics and are
not added to the official benchmark samples.

| Allocator setting | Peak RSS MiB | Wall seconds | RSS reduction | Exact output |
|---|---:|---:|---:|---|
| default | 1,091.71 | 70.86 | 0.00% | True |
| lazy-arena | 1,065.23 | 72.53 | 2.43% | True |
| purge-immediate | 1,026.58 | 74.02 | 5.97% | True |

`lazy-arena` sets `MIMALLOC_ARENA_EAGER_COMMIT=0`; `purge-immediate` sets
`MIMALLOC_PURGE_DELAY=0` and `MIMALLOC_PURGE_DECOMMITS=1`. Every control enables
`MIMALLOC_SHOW_STATS=1`. These settings save only 2.4–6.0% and increase wall time
in these single samples. Allocator reservation or retention therefore contributes,
but these controls do not explain or remove the roughly 2.7× Rust/C++ RSS gap.
The System-allocator diagnostic likewise retains nearly the same live payload.

Dropping all transformed results immediately before merging releases about
68 MiB of live allocations in `t2`, but its measured peak RSS remains about
1,093 MiB: the large contribution list is already live before the drop.
The peak requested payload decreases only from 887.44 to 875.31 MiB.
This is a secondary lifetime cleanup, not the main memory fix.

## Mitigation prototypes

All prototype edits live in an isolated source copy under
`target/hf-memory-cause-20260908/source/`. Production sources, vendor sources,
benchmark executables, running cases, and their memory limits were not changed.

1. **Collect finite terms incrementally.** Process each entry in original
   encounter order and immediately feed its finite terms into the existing
   balanced coefficient-sum strategy keyed by exact structural equality.
   Consume transformed-result references as each entry finishes. This avoids
   keeping the full raw list and the second full-size merge vector.
   The prototype is deliberately limited to serial execution with divergence
   checking disabled, matching the tested attachment requests. It still performs
   all transforms first, preserves coefficient encounter order within each key,
   and retains final canonical output ordering. A production implementation
   needs corresponding bounded collection for boundary bins and ordered parallel
   batches, with regression coverage for those paths and algebraic-letter indices.
2. **Box the lazy compatibility-view payload.** Preserve shared lazy views but
   avoid reserving both `Poly` structs in every uninitialized holder. This changes
   storage only; the public rational-function values and view behavior are preserved.
3. **Keep allocator tuning optional.** Immediate purging is a small measured
   memory/time tradeoff, and changing it globally is not justified by one case.
4. **After streaming, profile the next peak.** Candidate follow-ups are sharing
   immutable word spines, reusing context-local exact constants, consuming inputs
   once their last use ends, and bounding transform caches by bytes when required.
   Rat clones already share their polynomial payloads, the period scratch cache
   is already bounded, and recursive transform caches are already dropped before
   endpoint processing. Reimplementing those existing measures would miss the issue.

The diagnostic builds use Rust 1.89, release optimization, line information,
16 codegen units and no LTO, plus live-byte counters. Compare prototype timings
with the **instrumented** baseline, not the official release speedup. Single
samples demonstrate footprint and exact-output behavior, not a stable speed claim.

| Case | Diagnostic variant | Peak RSS MiB | Wall seconds | Exactly matches archive |
|---|---|---:|---:|---|
| `tst1` | archive | 71.27 | 3.87 | True |
| `tst1` | instrumented | 70.84 | 4.62 | True |
| `tst2` | instrumented | 1,093.09 | 89.50 | True |
| `tst2` | drop-transforms | 1,093.39 | 88.12 | True |
| `tst2` | system | 1,058.65 | 101.50 | True |
| `tst0` | archive | 22.52 | 0.14 | True |
| `tst0` | boxed-stream | 22.58 | 0.17 | True |
| `tst1` | boxed | 65.98 | 4.37 | True |
| `tst1` | boxed-stream | 45.62 | 4.29 | True |
| `tst2` | boxed | 948.36 | 82.58 | True |
| `tst2` | boxed-stream | 105.15 | 81.72 | True |
| `parity_face` | archive | 167.85 | 17.14 | True |
| `parity_face` | boxed-stream | 166.85 | 19.12 | True |
| `parity_face` | allocator-tuned | 95.45 | 22.83 | True |

`boxed` changes only lazy-view storage relative to the instrumented baseline.
`boxed-stream` adds incremental finite-term collection and consumes transformed
references. `drop-transforms` is the separate early-drop control on the original
representation. `system` substitutes the allocator in the diagnostic binary.

After incremental collection, the largest observed `tst2` requested-live peak is
**74.57 MiB**, reached while holding transformed entries before dropping their
recursive caches; it was 887.44 MiB before the mitigations. The next useful
memory target on this case is therefore the transform phase.

`parity_face` behaves differently: the combined prototype changes RSS only from
167.85 to 166.85 MiB. Its requested-byte high-water mark reaches **78.11 MiB**
in the early `x2` step even though only 14 raw finite terms are produced there,
and live payload is back to 6.37 MiB when those entries are collected. That
points to transient per-entry arithmetic/collection work as an additional target;
the phase data does not identify the specific polynomial operation. It calls for
per-operation profiling, rather than assuming the large-raw-list mitigation will
solve every case. The separate production-binary `allocator-tuned` control on
`parity_face` reduced RSS by **43.13%**, at a **33.16% wall-time increase** in this
single pair. It combines `MIMALLOC_ARENA_EAGER_COMMIT=0`, `MIMALLOC_PURGE_DELAY=0`
and `MIMALLOC_PURGE_DECOMMITS=1`; compare it with that case's `archive` row.
This supports per-case allocator tuning when memory is more important than latency.

## Scope and reproducibility

The **20.37 GiB** successful Rust peak on `tst3` is established by the original
benchmark, but this investigation directly attributes allocations on `tst1`
and `tst2`. The shared pipeline and the similar workload scaling make the same
mechanism plausible on `tst3`; exact savings there require a separate run.
No percentage savings is extrapolated to `tst3`, `tst4`, or the quadruple boxes.
Unfinished cases can have different bottlenecks. The measured fall in live
allocations after collection supports large temporary working data as the
explanation here; this is not a general leak audit.

Artifact directory: `target/hf-memory-cause-20260908/`.

- `allocator-results.json`, `diagnostic-results.json`, `mitigation-results.json`:
  `wait4` process measurements, effective-request hashes, equality checks, and raw paths.
- `stages.csv` / `stages.json`: phase counts and Rust requested live/peak bytes;
  RSS readings are observational `/proc` samples. Official per-run peaks above
  use `wait4`, rather than sampled `VmHWM` values.
- `rust-tst1-resolved.heaptrack.zst`, `rust-tst1-resolved.peak-stacks`,
  `heap-summary.json`: successful allocation-site evidence and aggregation.
- `instrumentation.patch` and `mitigation-prototypes.patch`: changes relative
  to the archived sources and instrumented baseline respectively.
- `experiment-manifest.json`, build logs, and runner scripts: source provenance,
  compiler choices, allocator environment overrides, and reproduction details.

The successful heaptrack run uses a copy of the instrumented binary with a
compatible glibc 2.42 loader and System allocator; its timings and RSS include
profiler overhead and are not benchmark samples. Earlier loader-incompatible
or unsymbolized attempts, and empty C++ traces from its statically overriding
mimalloc, are excluded from allocation attribution. The main run stayed on
CPU196; diagnostic runs used CPU194, profiling CPU193, and builds CPUs188–191.
