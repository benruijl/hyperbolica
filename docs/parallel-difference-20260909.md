# Why parallel Hyperbolica scales differently from HyperFLINT

The main measured scheduling difference is the Rust integration pipeline: it admits only twice the worker count ahead of a single ordered coefficient collector. An unfinished early entry, or a slow coefficient merge, can stop admission while other workers are available. HyperFLINT dynamically schedules the full entry list, uses worker-local contribution buckets, and merges independent keys in parallel.

This is an architectural difference in the port, rather than evidence that Symbolica arithmetic is inherently slower on eight cores. Rust often consumes fewer CPU-seconds. CPU/cache placement and shared-host load also change the observed backend ranking substantially.

## Fresh uninstrumented controls

One adjacent pair per case/mode, same physical CPUs for both backends: CPU64 for serial and CPUs64–71 (one shared L3 cache) for eight workers. Release executables, fresh processes, no warmup, 256 GiB address space, 256 MiB stacks, 24-hour timeout. These are single-pair observations, not medians or replacements for the ongoing full suite. Wall time includes startup, parsing and serialization. All normalized outputs match the already-verified corresponding suite result.

| Case | Workers | HyperFLINT wall s | Hyperbolica wall s | CPU s FLINT / Rust | Busy cores FLINT / Rust | Peak MiB FLINT / Rust |
|---|---:|---:|---:|---:|---:|---:|
| tst2 | 1 | 136.416 | 101.049 | 134.80 / 99.57 | 0.99 / 0.99 | 403.63 / 84.43 |
| tst2 | 8 | 20.236 | 12.822 | 155.06 / 72.42 | 7.66 / 5.65 | 474.95 / 182.89 |
| tst3 | 8 | 314.737 | 222.517 | 2427.55 / 1257.08 | 7.71 / 5.65 | 7850.52 / 1003.86 |

The current uninstrumented pairs favor Rust on these shared-L3 cores. They do not establish a universal ranking. The tst2 serial and parallel CPU times also differ substantially (99.57 versus 72.42 s for Rust); a scaling factor from this single pair should not be interpreted as a stable measure of parallel efficiency.

The older full-suite eight-core affinity spans two L3 caches (CPUs1,2,3,5,6,7,8,9), and its pinned Rust binary predates the rational-handle change. Its three-pair tst3 medians are 302.991 s for HyperFLINT and 345.647 s for Rust. Comparing that table directly with a new build on different cores confounds implementation, placement and host load.

## What the implementations do differently

| Stage | HyperFLINT | Hyperbolica |
|---|---|---|
| Transform | Inside each worker’s entry body | Serial, shared transform session/cache before entry processing |
| Admission | OpenMP dynamic schedule, one entry at a time from the full input | Rolling window of 16 uncollected entries for eight workers |
| Contribution storage | Worker-local buckets; colliding coefficients normally deferred | One ordered collector; balanced sums combine coefficients incrementally |
| Final coefficient merge | Independent keys processed in parallel, tree merge within each key | Collector finish and final canonicalization are serial |
| Memory policy | Retains more pending contributions | Bounds pending entries and reduces partial sums eagerly |

HyperFLINT source is the pinned `adfd3af3be234cb43a2322bd9ec442caa26edd74` checkout. Relevant source: `HyperFLINT/src/integrator/integration_step.cpp` (entry dispatch, worker accumulators and parallel key merge), `include/hyperflint/integrator/poles_bucket.hpp` (deferred contributions). Rust counterparts are `src/integrator/integration_step/driver.rs`, `ordered_parallel.rs`, `src/integrator/transform/collection.rs`, and `src/integrator/accumulator.rs`.

There is also a cache-reuse asymmetry. Rust keeps a recursive word/subword `TransformSession` across the whole step. HyperFLINT’s default `transform_word` creates a fresh recursive cache per call; its optional outer operator-memo cache is disabled in these baseline environments. In the tst3 traces, Rust transformation totals 11.39 serial wall-seconds, while HyperFLINT records 554.88 summed worker-wall-seconds in transformation. Those units differ, and both underlying arithmetic and cache reuse contribute; the measurements do not isolate the cache benefit alone. Preserve the Rust reuse when increasing parallelism.

Both use eight outer workers in these parallel runs. HyperFLINT arithmetic is limited to one inner thread; the Rust diagnostic confirms eight licensed workers. Both use mimalloc. A HyperFLINT tst3 cycle profile attributes only 0.14% to libgomp, so its high CPU occupancy is not predominantly OpenMP spin waiting. Sampled Rust hotspots are polynomial arithmetic, allocation/freeing, copying and hashing; no synchronization symbol exceeds the flat report’s 0.3% cutoff. The Rust profile covers the latter part of the run, not all phases.

## Queue experiment

A separate diagnostic build records entry and collector activity and accepts a lookahead override. It preserves processing, input order and coefficient summation. The production sources and ordinary release executable were restored after building it. Only the queue window changes between the diagnostic variants.

| Case / trial | Window | Diagnostic wall s | CPU s | Peak MiB |
|---|---:|---:|---:|---:|
| tst2 / initial | 16 | 16.605 | 82.62 | 180.86 |
| tst2 / initial | 128 | 13.372 | 85.95 | 190.89 |
| tst3 / with partial perf | 16 | 228.420 | 1245.67 | 1017.62 |
| tst3 / initial | 128 | 211.647 | 1502.22 | 1015.65 |
| tst3 / repeat, no perf | 16 | 261.681 | 1419.09 | 1014.62 |

The 16-entry tst3 repeat changed from 228.42 to 261.68 s, but the largest-step activity deficit persisted (5.95 and 5.83 active cores, versus 7.97 with 128 entries). The wider-window run consumed 1502.22 CPU-seconds versus 1245.67–1419.09 for the two narrow-window runs. Peak tst3 RSS stayed close to 1 GiB in all three trials; tst2 RSS rose by about 10 MiB. Increased concurrency can change cache behavior and working-set cost, but no paired cache-miss/instruction-counter experiment was performed, so the source of the CPU-time increase is not isolated.

Diagnostic totals include JSON trace emission after each step; the pipeline timings below exclude that emission. Cycle profiling was attached to the first tst3/window16 run partway through execution. The repeat runs without perf. These totals are diagnostic evidence and should not be presented as production speedups.

For the largest step (integration over t2), activity intervals give:

| Case / trial | Window | Processing pipeline s | Active entry/collector cores | Collector s | Idle core-s: earlier entry | Idle core-s: collector admission |
|---|---:|---:|---:|---:|---:|---:|
| tst2 / initial | 16 | 8.789 | 5.809 | 2.938 | 13.055 | 5.893 |
| tst2 / initial | 128 | 6.450 | 7.908 | 3.524 | 0.025 | 0.477 |
| tst3 / initial | 16 | 137.921 | 5.951 | 60.123 | 155.070 | 125.533 |
| tst3 / initial | 128 | 119.366 | 7.966 | 79.909 | 0.000 | 3.657 |
| tst3 / repeat | 16 | 173.991 | 5.825 | 70.512 | 226.668 | 148.897 |

Idle core-seconds are unused application worker capacity integrated over time while entries remain unissued and the credit window is full. They are not consumed CPU cycles. “Earlier entry” means the ordered collector cannot take the next result; “collector admission” means collection is active but the remaining workers cannot advance beyond the window. Activity intervals include OS descheduling, so these measurements do not imply that all such capacity would have been available on the shared host. They are not a directly attainable wall-time saving.

For the initial tst3/window16 trace, serial transformation totals about 11.39 s, collection totals 77.72 s (overlapping entry processing), and the final per-step merge totals about 1.08 s. Thus parallelizing transformation alone or merely parallelizing the final collector finish would miss most of the measured admission bottleneck.

HyperFLINT’s traced largest tst3 step spends 190.61 s in the parallel entry region, with each worker accumulating 190.22–190.31 s of entry-body activity. Its post-entry coefficient canonicalization takes 14.14 wall-seconds while accumulating 112.71 worker-wall-seconds across almost eight fully active workers. Rust’s ordered collector takes 60.12 wall-seconds in this step, overlapping entry processing but using only one worker at a time. This illustrates how doing less coefficient work can still create a longer critical path. HyperFLINT’s merge consumes 14,204,385 input monomials and emits 5,714, whereas Rust compresses partial sums incrementally. The corresponding Rust trace shows the admission stalls directly. Phase boundaries differ between implementations, so their individual sub-timers are not interchangeable arithmetic microbenchmarks.

## Next changes to prioritize

1. Use adaptive lookahead governed by retained bytes as well as entry count. Allow workers to pass a slow early entry when memory permits; keep an upper bound for large qbox/tst4 contributions. A fixed 128-entry limit is only an experiment, not a universal setting.
2. Partition coefficient collection across independent structural keys. Retain deterministic input order and the same balanced summation within each key, while different keys combine concurrently. This removes the single collector from the admission critical path without retaining every raw contribution until the end.
3. Measure cache/working-set cost alongside utilization. Larger windows can improve occupancy while increasing CPU time and peak memory. Validate the combined policy with repeated paired tst2/tst3 runs and memory checks on the larger cases.

Raw inputs/outputs, phase events, scheduler traces, cycle profiles, source and executable identities are under `target/parallel-difference-20260909/`. The three ordered-pipeline tests passed for the diagnostic build; every diagnostic and control output matches its archived expected result. Production Rust source hashes match the pre-investigation snapshot. The full suite continues separately on its original pinned executable.

[Full suite](full-suite-20260908.md) · [Previous ownership optimization](multicore-overhead-20260908.md)

Uninstrumented Rust SHA-256: `08a9007b2fc8bd6923787236a37804fd8125466dc36723c69dd39c6f709663da`.
HyperFLINT executable SHA-256: `8140b11d4628defa83301a1e37790b1af78a7bce1da3dc0d5a17ec17c723d8b1`.
Diagnostic Rust SHA-256: `edb6674b5585ed231e55e3068d18d83649d56bbd038e7bf1add83fa3cd30a8c1`.
