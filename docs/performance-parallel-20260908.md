# Parallel backend comparison, 2026-09-08

> Earlier benchmark jobs and report watchers were stopped by user request.
> A fresh complete-suite run is tracked in [the new live report](full-suite-20260908.md).
> Running/queued labels below belong to the preserved earlier snapshot.

Updated 2026-09-08T19:10:53.654806+00:00.

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

The [complete current report](performance-optimized-20260908.md) also tracks
the concurrent eight-core `tst4` and queued quadruple-box comparisons.
