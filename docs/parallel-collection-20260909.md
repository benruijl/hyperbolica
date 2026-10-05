# Parallel coefficient collection and adaptive lookahead

This change addresses the two bottlenecks measured in [the parallelism investigation](parallel-difference-20260909.md): the single coefficient collector and admission stalls behind an unfinished early entry.

## Implementation

Finite contribution keys are distributed across independent collectors (four shards per worker, capped at 64). A key always receives its coefficients in the original encounter order and retains the existing balanced incremental sum. Entries with at least 256 terms dispatch the shards through Rayon; smaller entries avoid that dispatch. Finishing the partial sums also runs across shards. Global first-encounter positions restore the original output order. Digest collisions still require full structural key equality, and failures retain their original precedence.

The entry scheduler starts with two outstanding entries per worker and can grow to sixteen per worker when estimated retained storage permits. With eight workers this is 16–128 entries. Reservations include queued jobs, running jobs, completed results, and the result being collected. This prevents queued work from silently exceeding the entry bound. Completed workers can admit another entry while the ordered collector waits for an earlier result.

Extra admission uses a 64 MiB storage target: the individually estimated sizes of completed results plus headroom at the largest observed result size for each unfinished job and the proposed new job. Completed small results are therefore not all charged as though they were the largest result. Large results suppress growth above the initial window. The initial window always permits progress, even when a single contribution exceeds the target. This is a backpressure estimate, **not a hard RSS limit**: arithmetic scratch, active results of unknown size, shared transform storage, and accumulated boundary terms are outside that estimate. Shared rational buffers are conservatively charged per reference. The entry count is the hard bound.

The existing step-wide transform cache remains in place. Boundary contributions retain their ordered handling, and serial execution uses the original collector. This change does not retain all raw contributions for a final merge.

## Validation

All-target tests, Clippy with all features and warnings denied, and the pure-Symbolica dependency gate pass. New tests cover passing a slow first entry, bounded live/queued work, progress over the storage target, nested/single-worker execution, ordered failures, cancellation, canonical key ordering, mixed contexts, and forced digest collisions. Collection is checked with one, two, and eight workers and several batch sizes against the serial collector.

## Release measurements

Fresh sequential processes on CPU64 for serial and CPUs64–71 for eight workers (one shared L3). The Rust comparisons use three alternating adjacent pairs per parallel case; the serial check and each HyperFLINT control use one sample. All runs have a one-hour timeout, 256 GiB address-space limit and 256 MiB stacks. There is no instrumentation or warmup. The original suite uses separate CPUs and remains pinned to its older binary. Other host workloads remain a source of variance.

Wall time includes startup, parsing and serialization. Values below are medians for time/CPU occupancy and maxima for peak RSS. HyperFLINT controls are single observations, not three-sample medians.

| Case | Workers | Previous Rust s | New Rust s | Speedup | HyperFLINT s | Peak MiB previous / new / FLINT |
|---|---:|---:|---:|---:|---:|---:|
| attachment.tst2 | 8 | 14.916 | 11.283 | 1.322× | 18.269 | 218.88 / 198.88 / 464.46 |
| attachment.tst3 | 8 | 227.623 | 223.353 | 1.019× | 243.483 | 1027.68 / 1040.68 / 7893.66 |
| attachment.tst2 | 1 | 72.599 | 70.215 | 1.034× | — | 84.71 / 84.23 / — |

| Case | Workers | CPU s previous / new | Busy cores previous / new | Paired speedup range |
|---|---:|---:|---:|---:|
| attachment.tst2 | 8 | 79.32 / 80.09 | 5.33 / 7.13 | 1.294–1.425× |
| attachment.tst3 | 8 | 1250.00 / 1315.29 | 5.50 / 5.89 | 0.892–1.279× |
| attachment.tst2 | 1 | 71.83 / 69.26 | 0.99 / 0.99 | 1.034–1.034× |

All 16 main-comparison outputs match the corresponding archived result after excluding declared non-semantic fields. Effective request hashes match within every comparison. All 162 archived corpus requests also match exactly in a separate correctness-only run (eight-worker budget, CPU244 affinity).

All three tst2 pairs improve. The tst3 paired ratios cross 1: its small difference between medians does not establish a consistent speedup on this shared host. The measured change combines parallel collection with adaptive admission; it does not isolate their individual contributions.

The resource sampler also records each thread’s cumulative runnable wait from `/proc/.../schedstat`. These sampled totals are included in the CSV. They measure one source of shared-host interference; they do not isolate cache, frequency, memory-bandwidth or algorithmic effects.

These measurements cover tst2/tst3. A completed qbox/tst4 run with the new collector is not yet available, so the bounded scheduler tests do not establish their final peak RSS or speedup.

## Separate placement check

The main CPU group has online SMT siblings (CPUs320–327) that were not reserved. One additional adjacent Rust pair uses CPUs224–231, one L3 group without online SMT siblings. It runs concurrently with the main comparison on a separate CPU/cache group; the host remains shared. This single pair is a placement check, not a replacement for the main samples or a controlled isolation of SMT effects. Both outputs match.

| Case | CPUs | Previous Rust s | New Rust s | CPU s previous / new | Busy cores previous / new | Peak MiB previous / new |
|---|---|---:|---:|---:|---:|---:|
| tst3 | 224–231 | 224.463 | 243.224 | 1195.11 / 1249.44 | 5.32 / 5.14 | 1000.12 / 1036.81 |

Sampled cumulative runnable wait rises from 115.34 to 302.73 thread-seconds in this pair. Removing online SMT siblings did not remove shared-host scheduling interference. This result reinforces the lack of a consistent demonstrated tst3 speedup; it does not isolate a code regression or an SMT effect.

## Reproducibility

- baseline: `/common/dev/hyperbolica/target/multicore-inline-20260908/candidate`, SHA-256 `08a9007b2fc8bd6923787236a37804fd8125466dc36723c69dd39c6f709663da`.
- candidate: `/common/dev/hyperbolica/target/parallel-collection-20260909/candidate-v2`, SHA-256 `c47afb9d4964b6e8a2608d527c7ef95167540d555411a268d6bac9d42b39b1a1`.
- flint: `/common/dev/hyperbolica/target/cross-audit-20260907/hyperflint-cpp`, SHA-256 `80265b2ac13d4062d33eb25845dd5e81752e62d186a40631f9ebd53e7acffeb5`.
- flint_native: `/tmp/hyperflint-attachment-release-portable/hyperflint`, SHA-256 `8140b11d4628defa83301a1e37790b1af78a7bce1da3dc0d5a17ec17c723d8b1`.

Raw measurements, resource samples, normalized-output checks, source manifests, the incremental implementation diff and runnable comparison scripts are in `target/parallel-collection-20260909/`. The full per-run table is [the evidence CSV](benchmark-evidence/parallel-collection-20260909.csv).
