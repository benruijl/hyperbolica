# Peak-memory analysis, 2026-09-08

Snapshot at **2026-09-08T12:44:35.244725+00:00**. Benchmark execution was left running.
The underlying run continues in [the full attachment report](hf-benchmarks-20260908.md).

HyperLica's largest completed-case peak is **20.37GiB** on `tst3`,
versus HyperFLINT's **7.35GiB**. That is **2.77×** the peak RSS
and **13.02GiB extra**, in exchange for a **1.59×** runtime advantage
in the single timed pair. Both outputs compared exactly and repeated their
respective successful preflight outputs. The original 8GiB allocation failure
was a consequence of the imposed budget, not evidence that `tst3` was unsolvable.

## Completed-case peaks

Peaks below are the maximum over timed invocations, matching the main benchmark
report. They are not a sum of resident memory from concurrent jobs.

| Case | Timed pairs | HyperFLINT MiB | HyperLica MiB | RSS ratio Rust/C++ | Runtime speedup C++/Rust |
|---|---:|---:|---:|---:|---:|
| `tst0` | 3 | 18.01 | 22.53 | 1.25× | 3.28× |
| `tst1` | 3 | 33.57 | 71.80 | 2.14× | 2.16× |
| `tst2` | 3 | 401.14 | 1,092.11 | 2.72× | 1.77× |
| `tst3` | 1 | 7,528.54 | 20,860.42 | 2.77× | 1.59× |
| `findroots21_a` | 3 | 30.21 | 14.48 | 0.48× | 8.16× |
| `findroots21_b` | 3 | 30.23 | 14.46 | 0.48× | 4.32× |
| `parity_face` | 3 | 59.18 | 174.82 | 2.95× | 1.46× |

HyperLica uses more RSS in 5/7 completed cases. Its footprint is about half
of HyperFLINT on both root-finding cases. The largest relative increase is
`parity_face` (2.95×), but its absolute peak is only about 175MiB; `tst3` dominates
the actual memory requirement. A single average across these cases would hide
this difference in scale.

From `tst2` to `tst3`, C++ peak memory grows **18.77×** and Rust grows
**19.10×**. The Rust/C++ multiplier stays close to 2.7–2.8×.
This suggests a substantial memory cost alongside the runtime gains on this
workload family. The data alone cannot attribute it to polynomial representation,
intermediate retention, allocator behavior, or a leak. Allocation-site attribution
would require a separate profiling run.

## Repeatability of the largest peak

| `tst3` invocation | HyperFLINT peak GiB | HyperLica peak GiB |
|---|---:|---:|
| Exact preflight | 7.35762 | 20.36706 |
| Timed pair | 7.35209 | 20.37151 |

For each backend the two peaks differ by less than 0.1%. This supports a
repeatable workload footprint across these fresh processes; it is not a leak
test, a confidence interval, or an upper bound on later/harder inputs.

## Active process at capture

C++ `tst4` preflight, PID `4165139`, remained running on CPU196. Its
memory at this capture is provisional; the final per-process peak may be higher.

| Metric | Value |
|---|---:|
| Current RSS | 1.573GiB |
| Peak RSS so far (`VmHWM`) | 1.573GiB |
| PSS | 1.567GiB |
| Current virtual address space (`VmSize`) | 2.201GiB |
| Peak virtual address space (`VmPeak`) | 2.201GiB |
| Anonymous RSS | 1.563GiB (99.38% of RSS) |
| File-backed RSS | 9.94MiB |
| Mapped stack (`VmStk`) | 11.83MiB |
| Process swap | 0.00MiB |
| Host available memory | 724.0GiB |

The close RSS/PSS values and predominantly anonymous pages show that this
process’s live footprint is mainly private computation data, with little
shared-library contribution. This snapshot does not identify the allocating
data structures. The live stack mapping exceeds the old 8MiB stack limit,
corroborating the earlier stack-sensitive C++ crash. Raising the stack limit
to 256MiB did not allocate 256MiB of resident stack.

The 256GiB memory guard is **address space**, not RSS. Virtual reservations and
unresident mappings count against that guard. Current RSS, peak RSS, virtual
address space and available host RAM answer different questions; they should
not be substituted for one another. No memory pressure was apparent in this
capture, but the unfinished `tst4` and quadruple boxes cannot yet be sized.

## Measurement and ongoing observation

Completed peaks come from the existing helper’s `wait4().ru_maxrss`, converted
from Linux KiB to bytes. Each backend is launched through a fresh small helper
so the accumulating driver’s memory does not inflate backend RSS. Reported
peaks include parsing, computation, libraries and serialization; they do not
isolate live algebraic objects. Small-case ratios include substantial startup
memory. The retained `tst0`–`tst2` samples and new cases have the settings listed
in the main report; they are not relabeled as one homogeneous experiment.

A separate sampler on CPU195 now reads `/proc` every ten seconds and follows
the active backend across cases. It records RSS, the kernel high-water mark,
address space, anonymous/file RSS, mapped stack and available host memory.
It never signals, attaches to, restarts, or changes limits of a benchmark.
The single `smaps_rollup` capture provides PSS; it is not repeatedly page-walked.
The sampler’s own memory is outside the measured backend process. A very short
process can finish between samples; its official completed peak is still
retained by `wait4`. The sampler stops when the benchmark finishes or exits.

## Artifacts

Local files are under `target/hf-all-20260908/memory-analysis-20260908/`:

- `results-snapshot.json` and `live-snapshot.json`: frozen inputs for this analysis.
- `peak-memory.csv`, `peak-memory.png`, `peak-memory.svg`: completed-case data and plot.
- `live-trace.jsonl` and `live-latest.json`: continuing lightweight observations.
- `analyze.py` and `monitor.py`: reproducible analysis and nonintrusive sampler.
- `manifest.json`: hashes of the frozen inputs and produced artifacts.

The old `tst3-rust-resource-trace.csv` and `tst4-rust-resource-trace.csv` one
directory above describe only the **earlier capped/interrupted run**. They
are not full trajectories for the successful larger-budget `tst3` computation.

A follow-up [allocation-cause analysis and mitigation experiments](hf-memory-causes-20260908.md)
identifies the large raw-term retention and tests ways to reduce it.
