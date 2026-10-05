# All `hf_benchmarks.json` cases, 2026-09-08

> Earlier benchmark jobs and report watchers were stopped by user request.
> A fresh complete-suite run is tracked in [the new live report](full-suite-20260908.md).
> Running/queued labels below belong to the preserved earlier snapshot.

The archived comparison below uses the scalar-corrected HyperLica executable from the
[latest-dev report](performance-dev-20260908.md) and the same pinned HyperFLINT
reference. All 33 attachment cases are accounted for. **The larger-budget direct-case run is in progress.**

7 direct cases have completed exact comparison and all scheduled timed
pairs; 1 have finalized failed comparisons; 1 was interrupted by user request. The 23 SubTropica pipeline
inputs still require frontend preparation and supply no backend speed ratio.
This is not a completed 33-case end-to-end benchmark.

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


## Archived single-threaded benchmark conditions

## Budgets for hard cases

The original 8GiB address-space cap was an exploratory guardrail, not a suitable
capacity assessment for these deliberately hard inputs. At the budget change
the shared host had approximately 838GiB available out of 1.1TiB physical RAM.
The unfinished capped probe was stopped, and the four hard cases (`tst3`,
`tst4`, and both quadruple boxes) were restarted with **256GiB address space,
a 256MiB stack, and two hours per backend invocation**. The two backends run
sequentially on CPU196, with one algebra thread. No failed capped preflight is
reused in this larger-budget run.

Each hard case gets fresh exact preflight followed, if successful, by one fresh
adjacent timed pair, without a warmup. This provides a single-pair probe rather
than a robust timing estimate. The other cases use three alternating timed
pairs and one warmup. Every timed output is checked against its own preflight.
Failed or incomplete comparisons receive no speed ratio.

## Archived direct-input measurements

Times are fresh-process wall-time medians. Ratio is the geometric mean of
individual HyperLica/HyperFLINT pair ratios; below 1 favors HyperLica. Process
startup, parsing, computation and serialization are included. Peak RSS is the
maximum over the timed samples.

| Case | Status | Scheduled pairs | HyperFLINT s | HyperLica s | Paired ratio | Peak MiB C++ / Rust |
|---|---|---:|---:|---:|---:|---:|
| `tst0 *` | measured | 3 | 0.451 | 0.137 | 0.305 | 18.012 / 22.527 |
| `tst1 *` | measured | 3 | 8.003 | 3.696 | 0.464 | 33.566 / 71.797 |
| `tst2 *` | measured | 3 | 123.741 | 70.686 | 0.565 | 401.137 / 1092.109 |
| `tst3` | measured | 1 | 1887.047 | 1183.816 | 0.627 | 7528.543 / 20860.422 |
| `tst4` | user_interrupted | 1 | — | — | — | — / — |
| `findroots21_a` | measured | 3 | 0.119 | 0.015 | 0.122 | 30.211 / 14.477 |
| `findroots21_b` | measured | 3 | 0.043 | 0.010 | 0.231 | 30.227 / 14.461 |
| `parity_face` | measured | 3 | 25.252 | 17.113 | 0.684 | 59.176 / 174.816 |
| `qbox_one_mass` | preflight_failed | 1 | — | — | — | — / — |
| `qbox_collaborator` | pending | 1 | — | — | — | — / — |

`*` retains the completed three-pair measurements from the earlier run:
`tst0` used 4GiB/60s, `tst1` 4GiB/180s, and `tst2` 8GiB/900s, all with an
8MiB stack. Their original samples and conditions are preserved; they are not
relabeled as larger-budget measurements. The newly measured root and parity
cases retain their original address-space/time bounds and use a 256MiB stack.
There is no aggregate combining these different measurement schedules.

## Heavy-case preflight outcomes

These are execution probes, including failed attempts, rather than the timed
pairs above. An allocation failure under RLIMIT_AS does not prove that the
host ran out of physical memory; a timeout is not a completed calculation.

| Case | Backend | Outcome | Elapsed s | Peak MiB | Timeout s / address-space MiB |
|---|---|---|---:|---:|---|
| `tst2` | HyperFLINT | ok | 126.152 | 400.47 | 900 / 8192 |
| `tst2` | HyperLica | ok | 70.169 | 1094.01 | 900 / 8192 |
| `tst3` | HyperFLINT | ok | 1884.829 | 7534.20 | 7200 / 262144 |
| `tst3` | HyperLica | ok | 1183.944 | 20855.87 | 7200 / 262144 |
| `tst4` | HyperFLINT | timeout | 7201.226 | 17942.21 | 7200 / 262144 |
| `tst4` | HyperLica | user_interrupted | 334.550 | 1902.16 | 7200 / 262144 |
| `parity_face` | HyperFLINT | ok | 25.007 | 59.09 | 600 / 8192 |
| `parity_face` | HyperLica | ok | 16.938 | 168.38 | 600 / 8192 |
| `qbox_one_mass` | HyperFLINT | timeout | 7200.127 | 29442.55 | 7200 / 262144 |
| `qbox_one_mass` | HyperLica | memory_limit_or_allocation_failure | 1827.762 | 118183.05 | 7200 / 262144 |

## Earlier capped diagnostics

The [separate capped report](hf-benchmarks-capped-20260908.md) preserves the
8GiB outcomes: C++ `tst3` timed out at 1800s, Rust `tst3` failed allocation at
658.5s, and both C++ quadruple boxes failed a 512MiB allocation. C++ `tst4`
crashed with the default 8MiB stack; a 64MiB-stack diagnostic instead ran to
its 10s timeout. These observations motivated the larger budgets; they do not
establish that either backend cannot solve the hard inputs. The unfinished
capped Rust `tst4` probe was interrupted, not classified as a backend failure.

## Pipeline inputs

These 23 cases need the same projective gauge, analytic continuation and
subtraction terms, and requested epsilon order before either backend runs.
No prepared backend requests were supplied. Available SubTropica source
checkouts do not supply a runnable Wolfram kernel or the missing configuration.
Raw epsilon expansion is not an equivalent replacement. The literal-zero
triangle also needs defined frontend handling of unused variables.

| Original index (zero-based) | Case | Status |
|---:|---|---|
| 10 | `dbox1m` | Requires SubTropica preprocessing |
| 11 | `three_loop_form_factor` | Requires SubTropica preprocessing |
| 12 | `tri_1L` | Requires SubTropica preprocessing |
| 13 | `bub_1L` | Requires SubTropica preprocessing |
| 14 | `sunset_2L` | Requires SubTropica preprocessing |
| 15 | `ss2L_mid` | Requires SubTropica preprocessing |
| 16 | `p3L_a` | Requires SubTropica preprocessing |
| 17 | `vac3L` | Requires SubTropica preprocessing |
| 18 | `vac4L` | Requires SubTropica preprocessing |
| 19 | `vac5L` | Requires SubTropica preprocessing |
| 20 | `box_1L` | Requires SubTropica preprocessing |
| 21 | `pent1L` | Requires SubTropica preprocessing |
| 22 | `lib10_equal_mass_bubble` | Requires SubTropica preprocessing |
| 23 | `lib10_massive_triangle` | Requires SubTropica preprocessing |
| 24 | `lib10_massive_box` | Requires SubTropica preprocessing |
| 25 | `lib10_theta` | Requires SubTropica preprocessing |
| 26 | `lib10_parachute` | Requires SubTropica preprocessing |
| 27 | `lib10_bubble_pentagon` | Requires SubTropica preprocessing |
| 28 | `lib10_sudakov` | Requires SubTropica preprocessing |
| 29 | `tladder_L1` | Requires SubTropica preprocessing |
| 30 | `tladder_L2` | Requires SubTropica preprocessing |
| 31 | `tladder_L3` | Requires SubTropica preprocessing |
| 32 | `tladder_L4` | Requires SubTropica preprocessing |

## Provenance and evidence

- HyperLica executable SHA-256: `cf72945dcada1632e31dca72e883ad91997d741e755b8050af335a16963ffea4`.
- HyperFLINT executable SHA-256: `8140b11d4628defa83301a1e37790b1af78a7bce1da3dc0d5a17ec17c723d8b1`.
- Original attachment SHA-256 recorded by the prior run: `909be0ef1bc030a00ea823190733329c228a183e8f8e55ff909c56000c461e3c`.
- The original attachment directory is unreadable by this session. All ten exact
  requests are replayed from the prior resolved inventory, with request hashes
  verified. This run does not claim to have revalidated the attachment bytes.
- Executable, runtime and comparison-harness hashes are verified against the
  previous report. The license is loaded privately and not recorded.
- Timed pairs use CPU196 on the shared AMD EPYC9754 host. Its SMT sibling list
  is a singleton; affinity does not reserve the core. The retained early cases
  had reference-only preflights running on CPU195 concurrently; all their
  actual timed pairs also used CPU196.
- Output files are capped at 64MiB; core dumps are disabled. Timeout terminates
  the process group. RLIMIT_AS bounds address space per process, not aggregate
  process-tree RSS. Ratios are exploratory, not a universal parity claim.

Local evidence lives under `target/hf-all-20260908/`:

- `generous/{results.json,inventory.json,summary.csv,overview.md,invocations.jsonl,raw/}`
  records resolved limits, individual invocations, comparisons and timings.
- `paired-licensed/` retains the earlier completed three-pair measurements and
  explicitly interrupted capped run. `reference-preflight/` retains C++ probes.
- `run-generous.py` is the replay driver, hashed in metadata;
  `report-generous.py` generates this report. Earlier drivers and diagnostics
  remain alongside them. Raw evidence is ignored and not included in a clone.

Reproduce the larger-budget replay locally with a fresh output directory:

```bash
. "$HOME/.config/symbolica/env.sh"
python3 target/hf-all-20260908/run-generous.py \
  --memory-mib 262144 --timeout-seconds 7200 --stack-mib 256 --cpu 196 \
  --output target/hf-all-20260908/generous-reproduction
```

The standard [matrix driver](performance-matrix.md) supports the original
attachment directly. Its checked-in limits remain the original capped policy;
the local replay records this run’s explicit larger-budget overrides.
