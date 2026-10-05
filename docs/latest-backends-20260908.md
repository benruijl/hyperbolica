# Latest HyperFLINT/Flint and Hyperbolica/Symbolica benchmarks

> Earlier benchmark jobs and report watchers were stopped by user request.
> A fresh complete-suite run is tracked in [the new live report](full-suite-20260908.md).
> Running/queued labels below belong to the preserved earlier snapshot.

Updated 2026-09-08T19:10:59.985278+00:00.

Seven smaller attachment cases use fresh adjacent eight-worker C++/Rust
pairs on identical physical cores. Symbolica includes both the rolling queue
and the polynomial scratch fix. Other host work continues; these single
pairs are observations, not confidence intervals. Thread counts are budgets,
not claims that every operation uses all eight workers.
Fresh outputs match each backend’s archived response under fixture exclusions.
The fresh root pairs also have identical keys and exact zero coefficient
differences; their algebraic definitions match their respective archives.

| Case | Workers | Flint s | Symbolica s | Flint / Symbolica | Peak MiB Flint / Symbolica | Status / Rust build |
|---|---:|---:|---:|---:|---:|---|
| tst0 | 8 | 0.366 | 0.084 | 4.385 | 59.043 / 31.602 | measured; scratch fix + rolling queue |
| tst1 | 8 | 1.957 | 1.271 | 1.540 | 92.016 / 147.836 | measured; scratch fix + rolling queue |
| tst2 | 8 | 19.652 | 16.983 | 1.157 | 480.535 / 198.867 | measured; scratch fix + rolling queue |
| tst3 | 8 | 258.328 | 281.874 | 0.916 | 7,897.348 / 1,347.676 | measured; scratch fix + rolling queue |
| parity_face | 8 | 5.656 | 6.105 | 0.926 | 87.277 / 317.133 | measured; scratch fix + rolling queue |
| findroots21_a | 8 | 0.124 | 0.014 | 8.942 | 30.203 / 14.445 | measured; scratch fix + rolling queue |
| findroots21_b | 8 | 0.043 | 0.010 | 4.513 | 30.270 / 14.523 | measured; scratch fix + rolling queue |
| tst4 | 8 | 4,314.398 | 7,367.829 | 0.586 | 141,355.035 / 18,674.355 | ok / ok (symbolically equal); earlier batch build |
| qbox_one_mass | 8 | 14,939.676 | 2,539.921 | — | 87,147.746 / 12,976.121 | running / running; scratch fix + rolling queue |
| qbox_collaborator | 8 | — | — | — | — / — | queued / queued; scratch fix + rolling queue |

A time ratio above one favors Symbolica. Running-job times are elapsed so
far; running-job RSS is provisional. Queued cases have no measured numbers.
`tst4` retains the latest completed pair from the earlier memory-optimized
batch build; it is not presented as a measurement of the scratch fix.
`tst4` outputs differ only in coefficient term ordering: the exact additive
term multisets agree, and parsing their difference gives zero. Its ratio
therefore uses exact symbolic equality, rather than identical output text.

The previous Rust `qbox_one_mass` failed at 2,335.597 s and 115.403 GiB RSS
on a 216.2 GiB allocation. Its failed result is preserved separately; the
qbox row above follows the fixed rerun. The collaborator Rust job is queued
to use the fixed build when its existing C++/Rust queue reaches that case.

## Fresh serial corpus

**162/162 cases completed**. Each completed case has one preflight,
one warmup, and three fresh timed samples per backend on CPU244, with
alternating timed pair order and original per-case resource limits. All
accepted outputs match each backend’s archived response under the existing
case-specific exclusions; cross-backend equivalence follows the archived
fixture comparisons, including declared semantic normalization.

Geometric mean Flint/Symbolica median-time ratio: **3.595×**.
Symbolica has the lower median in **162/162** completed cases.

[All serial corpus rows](benchmark-evidence/latest-corpus-20260908.csv).

[Allocation diagnosis and fix](qbox-allocation-20260908.md).
[Attachment measurement CSV](benchmark-evidence/latest-backends-20260908.csv).
[tst3 timing and utilization audit](tst3-timing-audit-20260908.md).

Artifacts, executable hashes, resource bounds and raw outputs are under
`target/qbox-allocation-20260908/`. Earlier binaries and measurements remain
pinned in their original artifact directories.
