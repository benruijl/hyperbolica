# Optimized benchmark statistics, 2026-09-08

> Earlier benchmark jobs and report watchers were stopped by user request.
> A fresh complete-suite run is tracked in [the new live report](full-suite-20260908.md).
> Running/queued labels below belong to the preserved earlier snapshot.

## Current optimized measurements

The [latest Flint/Symbolica table](latest-backends-20260908.md) tracks the
scratch-fixed build and current qbox runs. The completed measurements below
retain their earlier build identities; the collaborator Rust queue now selects
the scratch-fixed build.

Updated 2026-09-08T19:10:53.654806+00:00. Fresh Rust measurements cover all **162 previously completed
corpus cases** (486 timed samples), plus the separately measured `tst2`, `tst3`,
and `parity_face`: **165 completed serial cases** in total. Every completed
case matches its archived output, excluding the documented telemetry fields.
C++ figures are reused archived measurements; new timing comparisons are
unpaired. Original paired results remain historical evidence.

| Case | New Rust samples | Rust s | Old Rust MiB | New Rust MiB | Reduction | Reused C++ MiB | New Rust/C++ RSS |
|---|---:|---:|---:|---:|---:|---:|---:|
| `tst0` | 3 | 0.151 | 22.53 | 22.48 | 0.19% | 18.01 | 1.248× |
| `tst1` | 3 | 4.030 | 71.80 | 42.54 | 40.76% | 33.57 | 1.267× |
| `tst2` | 1 | 69.010 | 1092.11 | 100.38 | 90.81% | 401.14 | 0.250× |
| `tst3` | 1 | 1254.011 | 20860.42 | 1249.89 | 94.01% | 7528.54 | 0.166× |
| `findroots21_a` | 3 | 0.014 | 14.48 | 14.46 | 0.13% | 30.21 | 0.479× |
| `findroots21_b` | 3 | 0.008 | 14.46 | 14.51 | -0.32% | 30.23 | 0.480× |
| `parity_face` | 1 | 18.974 | 174.82 | 167.69 | 4.08% | 59.18 | 2.834× |

The latest `tst0`, `tst1` and root rows use three new samples on CPU244, with
an 8 MiB main stack. `tst2`, `tst3` and `parity_face` retain their verified
optimized single samples on CPU194 with a 256 MiB stack. C++ and old Rust
attachment columns come from the original full attachment run. Earlier
one-sample release checks remain in their raw evidence; the table uses the
latest available measurements and does not combine peaks across runs.

| Case | Workers | Wall s | Peak RSS MiB | Speedup over optimized serial |
|---|---:|---:|---:|---:|
| `tst2` | 2 | 54.886 | 111.59 | 1.257× |
| `tst3` | 8 | 389.183 | 1326.99 | 3.222× |

## Matched parallel HyperFLINT versus Hyperbolica

Fresh C++ and optimized Rust subprocesses run back-to-back on the same physical
cores: two workers for `tst2`, eight for `tst3`. Each completed output must
match the verified archived parallel result and its exact effective request.
The C++/Rust elapsed ratio exceeds one when Hyperbolica is faster.

| Case | Workers each | Status | HyperFLINT s | Hyperbolica s | C++/Rust time | Peak MiB C++ / Rust |
|---|---:|---|---:|---:|---:|---:|
| `tst2` | 2 | measured | 70.800 | 59.731 | 1.185 | 422.238 / 110.324 |
| `tst3` | 8 | measured | 305.585 | 434.655 | 0.703 | 7886.973 / 1333.113 |

One adjacent timed pair per case, C++ first, without warmup. Both use the same
worker count and CPU affinity, 256 GiB address space, 256 MiB main/worker
stacks, and 24-hour per-invocation timeouts. C++ uses outer OpenMP parallelism
with one FLINT arithmetic thread per worker; Rust uses the matching Rayon
thread budget. Other long benchmarks continue on separate cores of the shared
host. These are single-pair observations, not confidence intervals or a
universal speed claim. Earlier Rust-only parallel samples remain separate.

The [parallel comparison CSV](benchmark-evidence/parallel-comparison-20260908.csv)
and `target/hf-memory-implementation-20260908/parallel-comparison/` retain
individual runs, output paths, hashes, limits and the live CPU/RSS trace.

### Shared-core contention control

A two-second scheduler observation during the fresh Rust `tst3` run showed
6.562 CPU-seconds scheduled across its workers and 3.230 seconds of cumulative
runnable-queue wait. The selected cores were 82–100% busy, including unrelated
host work. The same affinity therefore did not reserve equivalent CPU capacity
throughout the pair. A second C++ sample was queued immediately after Rust
on those same cores to assess timing sensitivity to the later host load.

Follow-up C++ status: **ok**.

C++ before Rust: **305.585 s**; Rust: **434.655 s**; C++ after Rust: **303.239 s**.

Follow-up C++ peak RSS: **7905.86 MiB**. Exact output and effective request match: **True**.

The two C++ samples bracket one Rust sample. They are not two independent
paired repetitions and are not combined into a confidence estimate. Raw
control metrics and the scheduler trace are retained in
`parallel-comparison/tst3-cpp-control/`.

### Why the eight-core result reverses the serial result

The archived serial baselines were 1,887.047 s for C++ and 1,254.011 s for
optimized Rust: a 1.50× Rust advantage. Relative to those baselines, the fresh
eight-core pair scales by 6.18× for C++ and 2.89× for Rust. These baselines
were measured at different times and affinities, so this is an indicative
scaling comparison rather than a controlled scaling sweep.

In the fresh parallel pair, C++ consumed 2,034.577 CPU-seconds in 305.585 s,
whereas Rust consumed 1,473.215 CPU-seconds in 434.655 s. Thus Rust used less
CPU work but averaged only 3.39 busy cores, compared with C++'s 6.66. All
eight Rust workers accumulated CPU time; this was not a one-thread fallback.

The Rust integration driver has a serial transform phase, then processes
batches of twice the worker count (16 entries here). Each batch waits for all
its entries before serial ordered contribution insertion and incremental
coefficient collection; only then can the next batch begin. Uneven entry
costs and collection work can therefore leave workers idle. These are
plausible bottlenecks visible in the implementation, not measured phase-time
attributions. Shared-host scheduling contention also contributed some delay.

Potential improvements are larger or adaptive batches, overlapping bounded
worker output with collection, and parallel collection across independent
structural keys while preserving each key's deterministic summation order.
Phase profiling should establish which change has the greatest benefit.
The current Rust run still used approximately 83% less peak RSS than C++.

The newer [rolling-queue scheduling comparison](worker-utilization-20260908.md)
tracks the follow-up Rust build separately.


## Eight-core tst4 background runs

Both jobs use the same request with `parallel: true`, **eight distinct physical
cores per backend**, a **24-hour timeout**, a **256 GiB address-space allowance**,
and 256 MiB main/worker stacks. HyperFLINT uses OpenMP workers on
CPUs40,41,46,47,57,58,60,63; Hyperbolica uses Rayon workers on
CPUs67,69,70,82,83,84,87,91. C++ FLINT arithmetic is kept at one thread inside
each outer worker to avoid nested thread oversubscription. Rust's per-call
budget is eight. These are concurrent single samples on separate cores of a
shared host, not an adjacent paired benchmark.

| Backend | Status | PID | Elapsed s | Peak RSS MiB | Peak final? |
|---|---|---:|---:|---:|---|
| HyperFLINT C++ | ok | 3518722 | 4314.4 | 141355.04 | Yes |
| Hyperbolica Rust | ok | 3518785 | 7367.8 | 18674.36 | Yes |

Exact-comparison status: **symbolically_equal**. Observed C++/Rust elapsed ratio: **0.586×**; Rust/C++ peak RSS: **0.132×**.

The previous two-hour C++ timeout and deliberately interrupted serial Rust
`tst4` runs are historical attempts. No incomplete run supplies a completed
speed ratio. The original controller continues the remaining quadruple boxes.
The 23 inputs requiring SubTropica preprocessing still have no valid backend
measurements. Completed `tst4` results will be compared and incorporated
automatically by the background report updater.

## Eight-core quadruple-box background queue

Both `qbox_one_mass` and `qbox_collaborator` are queued for HyperFLINT and
Hyperbolica. One concurrent C++/Rust pair runs at a time alongside `tst4`;
the next pair starts automatically. Each backend gets eight distinct physical
cores, 24 hours, 256 GiB address space and 256 MiB main/worker stacks.
HyperFLINT uses CPUs3,4,5,8,10,11,12,13 (NUMA0); Hyperbolica uses
CPUs97,98,100,101,102,103,114,119 (NUMA3), separate from `tst4`.

| Case | Backend | Status | PID | Elapsed s | Peak RSS MiB | Peak final? |
|---|---|---|---:|---:|---:|---|
| `qbox_one_mass` | hyperflint | running | 3960567 | 14933.3 | 87147.75 | No |
| `qbox_one_mass` | hyperbolica | memory_limit_or_allocation_failure | 3960571 | 2335.6 | 118172.86 | Yes |
| `qbox_collaborator` | hyperflint | queued | — | — | — | No |
| `qbox_collaborator` | hyperbolica | queued | — | — | — | No |

`qbox_one_mass` exact-comparison status: **pending**. 

`qbox_collaborator` exact-comparison status: **pending**. 

The original single-core quadruple-box benchmark remains independent. New
eight-core timings are concurrent single samples, not adjacent timed pairs.
The [queue CSV](benchmark-evidence/qboxes-eight-20260908.csv), queue state and
per-backend raw evidence are updated as each job finishes.



## Refreshed 162-case corpus

Each case has one fresh preflight, one warmup and three fresh timed Rust
subprocesses on CPU244, one algebra thread, with its original address-space
and timeout limits. The archived C++ medians and maximum timed RSS are reused.
Only the existing case-specific telemetry exclusions are applied to exact
output verification. Two factor-table cases were rerun after applying their
already-declared recursive timer exclusions; initial telemetry-only rejections
are preserved. There are **162/162 verified cases and 486 accepted timed samples**.

| Group | Cases | Geometric mean new Rust / archived C++ median |
|---|---:|---:|
| Core | 14 | 0.327 |
| API/differential | 79 | 0.337 |
| Generated scaling | 65 | 0.340 |
| Light attachments | 4 | 0.250 |
| All refreshed cases | 162 | 0.335 |

The ratio above uses unpaired medians and is not a fresh paired speedup
estimate. New Rust RSS is numerically above C++ in **79/162**
cases; **7** exceed C++ by at least 10%.
RSS is lower than the archived Rust sample maximum in
**57/162** cases. Very small differences near
process startup are measurement variation, not meaningful allocator changes.
The old report's counts, memory ratios and 0.412 paired aggregate describe
the previous executable and are not current-build statistics.

## Evidence

- [Full refreshed corpus CSV](benchmark-evidence/optimized-corpus-20260908.csv): all 162 cases, medians, peaks, archived comparators and unpaired ratios.
- [Current attachment and scaling CSV](benchmark-evidence/memory-collection-20260908.csv).
- [Background tst4 CSV](benchmark-evidence/tst4-eight-20260908.csv): provisional/final peak flags and outcome status.
- [Implementation and regression coverage](memory-collection-20260908.md).
- [Original paired attachment benchmark](hf-benchmarks-20260908.md).

Optimized executable SHA-256: `fcfba27a4817ebcc06e4228232803c27318c901855511f68a32edff3a6cdf2dc`.
Local evidence is under `target/hf-memory-implementation-20260908/`:
`corpus-refresh/results.json`, per-invocation raw outputs, `latest-stats.json`,
`tst4-eight/{hyperflint,hyperbolica}/`, and `tst4-eight/comparison.json`.
The watcher logs only outcome changes to `watch-latest-stats.log`; live traces
are read-only `/proc` samples. Final peak RSS is obtained from `wait4`.
