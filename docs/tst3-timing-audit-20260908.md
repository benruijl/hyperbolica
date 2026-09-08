# tst3 timing audit, 2026-09-08

The reported times accurately measure the completed processes. They are one
adjacent pair on a shared host, so the observed 9.1% Symbolica wall-time deficit
is not a statistically established performance margin.

| Backend | External wall s | Internal compute s | Total CPU s | Average busy cores | Eight-core utilization |
|---|---:|---:|---:|---:|---:|
| HyperFLINT / FLINT | 258.328 | 258.188 | 1947.012 | 7.537 | 94.2% |
| Hyperbolica / Symbolica | 281.874 | 281.758 | 1615.273 | 5.730 | 71.6% |

`scripts/benchmark_process.py` measures monotonic elapsed time around the child
process and obtains user/system CPU time and peak RSS with `wait4`. Each backend
also reports its own compute timer. The external/internal differences are
0.140 seconds for FLINT and 0.116 seconds for Symbolica; startup and output
handling cannot explain the 23.546-second gap.

Both processes received the same effective request, eight-worker budgets,
256 GiB address-space limits and 256 MiB main/worker stacks. They ran sequentially
on CPUs 132, 135, 139, 141, 143, 148, 149 and 150. These are eight distinct
physical cores, not SMT siblings. Outputs match the verified fixtures.
FLINT used eight outer OpenMP workers with one inner FLINT thread; Symbolica
used eight Rayon workers. This compares the two applications' parallel paths.

Symbolica consumed 17.0% less aggregate CPU time, but averaged 24.0% fewer
busy cores. Its lower aggregate work therefore did not translate into lower
wall time. This is evidence of lower effective utilization, not proof that
all missing utilization comes from the implementation: external scheduling
contention can also lower this metric. The current pair has no per-thread
scheduler-wait trace that separates these causes. Earlier runs documented
contention, but that observation cannot quantify contention in this pair.

The rolling queue removes the previous batch barrier, but the initial transform
is still serial and a slow earliest entry can exhaust the bounded lookahead
window. Those remain plausible implementation limits. CPU time also includes
spinning and other parallel overhead; it is not a count of useful operations.

The archived serial comparison was 1887.047 seconds for FLINT and 1254.011
seconds for the earlier memory-optimized Symbolica build. It used different
cores, times and a different Rust binary. The fresh 162-case serial corpus
does not include the hard attachment `tst3`. Consequently, these records do
not establish the latest build's one-to-eight-core speedup.

A controlled scaling claim needs repeated, alternating backend order at one
and eight cores with the same pinned binaries and inputs, warmups, and host
contention monitoring. No new long comparison run was started for this audit;
the existing qbox jobs remain running.

The benchmark resume had overwritten its overall start timestamp. That metadata
was restored from the preserved initial record; every measured process duration
and CPU counter remains unchanged.

[Audited counters and identities](benchmark-evidence/tst3-timing-audit-20260908.json).
[Latest benchmark table](latest-backends-20260908.md).
