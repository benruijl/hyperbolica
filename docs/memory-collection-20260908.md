# Incremental finite-term collection implementation, 2026-09-08

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

Validated 2026-09-08T14:10:10.040648+00:00. Both measured mitigation ideas are now implemented in the
working tree. This follows [the allocation-cause analysis](hf-memory-causes-20260908.md).

`Rat` now stores its lazy compatibility polynomials behind a box inside the
shared once-initialized holder. Unused holders request 32 bytes instead of
152 bytes, including Arc counters. Clones still share one lazily initialized
pair of monic polynomial views. No public API or native rational value changes.

The integration driver consumes each serial entry's transformed reference and
immediately collects its finite endpoint terms. A common owned collector uses
exact structural keys, namespace/context checks, and the existing balanced
coefficient sum in encounter order. The public borrowed collection function
uses the same implementation. This avoids retaining the entire raw contribution
list and a second full-size finite-term merge vector.

Licensed parallel execution processes indexed batches of twice the available
worker count and inserts completed results in input order. This bounds the
number of pending entry results while preserving summation order across batch
boundaries. Algebraic-letter mode retains serial entry processing and its
original transform encounter order. Boundary-divergence bins retain their
original raw terms and contour-closure ordering; only finite terms are streamed.
Thus divergence-enabled cases can still retain large boundary bins. All
transforms are still computed before entry processing, so their footprint can
become the next peak after the finite-term retention is removed.

A pre-existing all-features Clippy warning was also fixed: the period scratch
ring's `u16` exponent now uses infallible conversion to `i32` instead of an
unreachable conversion-error branch.

## Validation

- 452 library tests and 16 integration tests passed on the final code.
- New coverage checks cancellation and revival across owned batches, forced
  hash collisions, foreign coefficient and word contexts, and divergence
  cancellation spanning multiple two-worker batches. Removing the cancelling
  entry still produces the expected structured infinity-divergence error.
- The lazy-view regression checks that clones made before and after first use
  share the same initialized numerator and denominator.
- Formatting, all-targets/all-features Clippy with warnings denied, dependency
  and vendor-source checks, benchmark smoke tests, and all nine examples passed.
- The normal release profile (Rust 1.89, LTO enabled, one codegen unit, default
  production features) built successfully. No diagnostic allocator or feature
  toggle is present in production code.

An initial combined extras command supplied a libtest-only argument to Criterion;
Criterion rejected that argument. Bench smoke tests were rerun successfully using
`RUST_TEST_THREADS=1` instead. This was a harness invocation issue, not a test failure
in the implementation. The initial Clippy diagnostic and successful corrected pass
are retained in separate logs.

## Release measurements

These are fresh subprocess measurements of the new production release binary.
Each output matches the archived pre-change executable exactly after removing
only `timing_compute_s`. The two-worker `tst2` result also matches its serial
reference. Each row is one sample, not an official paired speedup estimate.

| Case | Execution | Peak RSS MiB | Wall seconds | Exact archived output |
|---|---|---:|---:|---|
| `tst0` | Serial | 18.46 | 0.14 | Yes |
| `tst1` | Serial | 42.47 | 3.53 | Yes |
| `tst2` | Serial | 100.38 | 69.01 | Yes |
| `tst2` | 2 workers | 111.59 | 54.89 | Yes |
| `parity_face` | Serial | 167.69 | 18.97 | Yes |

The earlier original-executable `tst2` control used **1091.71 MiB**.
Measurements use CPU194 for serial runs and CPUs194/197 for the parallel run,
with the same 256 GiB address-space allowance and 256 MiB stack allowance as the
independent diagnostics. No allocator-tuning variables are applied.

The [machine-readable release measurements](benchmark-evidence/memory-collection-20260908.csv)
record each new sample separately.

## Rust tst3 rerun after implementation

Completed 2026-09-08T14:31:39.595701+00:00. One new, single-threaded Rust subprocess ran on
CPU194 with the same request, 256 GiB address-space allowance, 256 MiB stack,
and 7200-second timeout. The complete response matches both the archived Rust
and C++ timed responses exactly after removing only `timing_compute_s`; the
effective request SHA-256 also matches. No C++ rerun was made for this update.

| tst3 executable | Wall seconds | Peak RSS MiB | Peak RSS GiB |
|---|---:|---:|---:|
| HyperFLINT C++, previous timed sample | 1887.047 | 7528.54 | 7.352 |
| Rust, previous timed sample | 1183.816 | 20860.42 | 20.372 |
| Rust, smaller lazy views + incremental collection | 1254.011 | 1249.89 | 1.221 |

Relative to the previous Rust sample, peak RSS decreased **94.01%**
and elapsed time increased **5.93%**. Dividing the reused C++
time by the new Rust time gives **1.505×**; new Rust/C++ peak RSS is
**0.166×**. These are comparisons of individual samples taken
at different times and on different cores (old CPU196, new CPU194), not a new
adjacent paired benchmark or confidence estimate. The shared host was also
running the original `tst4` benchmark during this rerun. During the latter
portion, the replacement optimized `tst4` and original quadruple-box benchmark
ran on CPUs197 and196 respectively. Affinity does not reserve these shared cores.

New executable SHA-256: `fcfba27a4817ebcc06e4228232803c27318c901855511f68a32edff3a6cdf2dc`.
Raw output, timing/RSS, both archived comparators, exact-equality checks and
request hashes are retained in `target/hf-memory-implementation-20260908/tst3-rerun.json`.
The read-only ten-second RSS trace is `tst3-memory-trace.jsonl`; reported peak
RSS comes from `wait4`, so short peaks between trace samples are included.

## Existing tst4 run

At validation, C++ `tst4` PID4165139 remained active on CPU196 and accumulated
199 CPU ticks during a two-second read-only observation. Its RSS was
6.37 GiB and it had no process swap.
Controller PID2134003 remained alive. The original archived Rust and C++
executables, source archives, and Cargo.lock still match their recorded hashes.
The existing attachment benchmark continues with its pinned executables and
settings; these new release measurements are separate from that original run.

Artifacts are under `target/hf-memory-implementation-20260908/`: `release-results.json`,
raw per-run stdout/stderr, build/test/lint/example logs, `validation.json`,
`verify-release.py`, and a preserved copy of the verified release executable.

At 14:20:13 UTC, the original C++ `tst4` invocation reached its pre-existing
7200-second timeout (measured wall time 7201.226 s). The controller automatically
started the pinned Rust `tst4` invocation on CPU196, PID2736528. No benchmark
was manually interrupted or restarted. The C++ timeout is not a completed
result and supplies no speed ratio. `tst4-handoff.json` preserves this transition.

At the user's request, the original Rust `tst4` PID2736528 was stopped at
14:25:48 UTC after 334.550 s. The harness records `process_error` from SIGTERM;
this was a deliberate interruption, not a backend correctness failure.
The new production build started `tst4` in an independent background process
on CPU197, backend PID2819562, runner PID2819442, at 14:25:48 UTC. Its timeout
is **86400 seconds (24 hours)**, with 256 GiB address space and 256 MiB stack.
The original benchmark controller proceeds with the remaining quadruple boxes
on CPU196. The new `tst3` rerun continues separately on CPU194.

Background evidence is in `target/hf-memory-implementation-20260908/`:
`tst4-background.log` and `tst4-optimized/{launch.json,request.json,live-latest.json,memory-trace.jsonl,raw/}`.
`tst4-optimized/result.json` is written on completion and records wall time,
peak RSS and output paths. This run cannot claim an exact C++ `tst4` comparison
because the original reference timed out. `tst4-user-interruption.json`
records the explicitly requested replacement.

## Eight-core tst3 follow-up

Completed 2026-09-08T14:45:45.986970+00:00. The serial optimized `tst4` run was stopped by
user request before this experiment; its interruption is recorded separately.
The eight-core run used the same optimized executable and effective request
as serial `tst3`, changing only `parallel` from false to true. Full output
matches optimized serial Rust, original Rust, and original C++ after removing
only `timing_compute_s`.

| Optimized tst3 execution | Wall seconds | Peak RSS MiB | Average CPU cores |
|---|---:|---:|---:|
| Serial | 1254.011 | 1249.89 | 0.99 |
| 8 workers | 389.183 | 1326.99 | 3.81 |

Observed speedup is **3.222×**, reducing wall
time **68.96%**. Peak RSS is
**1.062×** the optimized serial sample.
All **8 workers** accumulated CPU time; the highest five-second
sample averaged **4.81 cores**. Average cores are total process CPU
time divided by elapsed wall time, including serial portions. There was no
cgroup CPU quota or recorded CPU throttling in the inspected cgroup hierarchy.

Settings: CPUs74–77 and83–86, eight distinct physical cores on NUMA node2;
`parallel: true`, `RAYON_NUM_THREADS=8`, `HF_MAX_THREADS_PER_CALL=8`;
256 GiB address-space allowance, 256 MiB stack, 7200-second timeout. This is
one fresh parallel sample against the earlier serial sample on CPU194, on a
shared host. Core affinity does not reserve hardware; this is an exploratory
comparison, not a controlled scaling curve. The remaining original benchmark
continued on CPU196. No new `tst4` run was started after this experiment.

Evidence: `target/hf-memory-implementation-20260908/tst3-eight/` contains
`launch.json`, `request.json`, `trace.jsonl`, `result.json`, and raw outputs.
`run-tst3-eight.py` and `report-tst3-eight.py` reproduce the measurement and
report logic. The [CSV](benchmark-evidence/memory-collection-20260908.csv)
contains both serial and parallel measurements.
