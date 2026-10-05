# Reducing rational-handle overhead in parallel integration

Ordinary `Rat` clones previously incremented three independent reference
counts: context, native value, and lazy compatibility views. The context
counter was shared by many otherwise independent worker operations.
The handle now owns one `Arc<RatInner>`, so a clone updates one value-local
counter. Context, native value and lazy views share the handle lifetime.
The native value is stored inline in that shared allocation, eliminating
one allocation for each newly constructed rational value. Identity operations
that preserve a different caller diagnostic context share the original owning
root without copying polynomial buffers or retaining chains of intermediate
contexts. Lazy views remain context-specific. This changes ownership, not
algebra or scheduling.

A context alias keeps the owning root and any views cached on that root
alive. Intermediate aliases and their cached views are released normally;
the regression tests exercise this lifetime distinction.

The diagnostic tst2 run spent about 0.83 seconds transforming entries and
5.19 seconds collecting contributions, with collection overlapping entry
processing. This did not support parallelizing the transform as the main
target. Profiles also showed sensitivity to cache placement and time in
allocation and reference-count handling.

## Fresh tst2 comparison

Three adjacent baseline/candidate pairs per configuration, alternating
build order, without a separate warmup. All outputs match. Wall times include
process startup, parsing and serialization. Times are medians; memory is the largest
peak among those samples. These measurements use the same physical CPUs
for both builds, with 256 GiB address space and 256 MiB stacks. They are
separate from the still-running full suite and do not replace its samples.

| Workers / placement | Before s | After s | Time reduction | Before CPU s | After CPU s | Peak MiB before / after |
|---|---:|---:|---:|---:|---:|---:|
| 8, two L3 caches | 17.460 | 17.225 | 1.34% | 93.476 | 94.054 | 194.89 / 174.82 |
| 8, one L3 cache | 13.951 | 12.861 | 7.81% | 78.696 | 71.980 | 202.92 / 202.94 |
| 1 | 73.908 | 73.478 | 0.58% | 73.099 | 72.732 | 101.81 / 83.75 |

Eight-core CPU sets: 64–67 plus 72–75 for two L3 caches; 64–71 for
one L3 cache. The serial comparison uses CPU64. CPU placement and host
load differ from the original full-suite table, so only the adjacent
before/after pairs above should be used to assess this change.

Wall and CPU time are reported separately to expose utilization and work.
The host remains shared, and scheduler-wait traces accompany the results;
these three-pair observations are not a universal speed guarantee.

The two-cache wall-time difference is only 1.34%, with no reduction in
median CPU time; it is too small to separate confidently from sample
variation. The one-cache trial shows a 7.81% median wall-time reduction
and 8.53% less median CPU time. Cache placement remains a larger effect
than this ownership change in these observations.

The ordered collection stage and bounded lookahead remain potential scaling
limits. This change reduces shared reference-count traffic; it does not
claim eightfold scaling or change the memory window.

Validation: 457 library tests plus 16 integration tests, all-target/all-feature
Clippy, the pure-Symbolica source gate, 162 archived corpus outputs, and all
18 timed tst2 outputs pass. A new concurrent test checks that cloned values
retain their context-specific lazy views after the original handles are dropped.
Another regression checks native sharing and release of intermediate alias contexts.

Baseline binary: `4f4106444fe9ad0a37ef96b2a52ad26c4c61b71571c7eed0bc8cbbac7cd1892c`.
Candidate binary: `08a9007b2fc8bd6923787236a37804fd8125466dc36723c69dd39c6f709663da`.

[Measurement CSV](benchmark-evidence/multicore-overhead-20260908.csv).
[Original pinned full suite](full-suite-20260908.md).

Paired timings, scheduler traces, source identities and validation logs
are under `target/multicore-inline-20260908/`. Initial profiles and the
earlier shared-native-pointer experiment are retained under
`target/multicore-overhead-20260908/`.

## Individual wall-time samples

| Workers / placement | Before samples s | After samples s |
|---|---|---|
| 8, two L3 caches | 18.356, 17.221, 17.460 | 17.248, 17.225, 15.256 |
| 8, one L3 cache | 14.117, 13.951, 13.436 | 12.861, 12.488, 14.072 |
| 1 | 75.491, 73.908, 73.493 | 71.742, 75.614, 73.478 |
