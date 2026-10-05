# Bounded parallel entry scheduling, 2026-09-08

> Earlier benchmark jobs and report watchers were stopped by user request.
> A fresh complete-suite run is tracked in [the new live report](full-suite-20260908.md).
> Running/queued labels below belong to the preserved earlier snapshot.

The integration step now uses a rolling queue in place of fixed batches. A
collected entry releases one slot immediately, so other workers can process
its replacement while ordered coefficient collection continues. This removes
the barrier between successive batches.

The queue retains the existing limit of twice the licensed worker count: 16
entries for eight workers. A fixed ring holds completed results. Workers
briefly lock scheduling metadata, then perform integration and collection
outside the lock. One worker at a time owns the collector; other workers
continue processing. Waiting for missing results never blocks a Rayon thread,
including when called from a worker or from nested parallel work.

Contributions enter the collector in input order, preserving coefficient
summation order and boundary cancellation. Processing and collection errors
are selected in that order. The serial and algebraic-letter paths retain
their existing behavior. The initial transform phase is still serial, and a
slow earliest entry can still exhaust the bounded lookahead window.

Validation completed: 455 library tests and 16 integration tests passed;
all-target/all-feature Clippy passed with warnings denied. New concurrency
tests cover crossing a batch boundary, overlapping collection and processing,
bounded live work, input-order output and errors, and nested single-worker
execution.

The fresh eight-core comparison completed with identical verified outputs.
Both builds used physical CPUs 132, 135, 139, 141, 143, 148, 149, 150,
256 GiB address space, 256 MiB main/worker stacks and a 24-hour timeout.

| Case / build | Wall s | CPU s | Average busy cores | Peak MiB |
|---|---:|---:|---:|---:|
| tst3 / baseline | 431.488 | 1455.960 | 3.374 | 1334.03 |
| tst3 / rolling | 321.814 | 1535.123 | 4.770 | 1341.98 |
| tst2 / rolling | 18.755 | 95.345 | 5.084 | 174.90 |

For `tst3`, baseline/new wall-time ratio: **1.341×**;
elapsed change: **-25.42%**;
peak RSS change: **+0.60%**.

One baseline/new comparison was run, with the baseline first and no warmup.
There was a roughly 7-minute build-completion gap between the two runs.
Other host work caused scheduler contention; this is an exploratory paired
observation, not a confidence interval. The earlier C++ run is not a fresh
paired comparison against this build. The `tst2` row is a separate eight-worker
correctness/performance sample, not paired with the earlier two-worker result.

All **162 archived corpus outputs** also match with an eight-worker budget.
That separate run was pinned to CPU 244 for correctness checks and is not
used for speed claims. Raw invocations, traces, binary/source hashes and
validation results are retained under `target/worker-utilization-20260908/`.
The [measurement CSV](benchmark-evidence/worker-utilization-20260908.csv)
records the completed samples.

C++ `tst4` completed successfully in 4314.398 s, with
138.042 GiB peak RSS. Rust `tst4` and C++ `qbox_one_mass`
remain running on their pinned previous binaries. Rust `qbox_one_mass` ended with an allocation failure
after 2,335.597 s, at 115.403 GiB peak RSS: it requested a single allocation
of 232,157,872,128 bytes under its 256 GiB address-space limit. The collaborator
quadruple-box pair remains queued behind the C++ one-mass run. These records
and the earlier full-corpus measurements remain separate from this build.
