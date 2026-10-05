# Capped attachment diagnostics, 2026-09-08

This follow-up uses the scalar-corrected HyperLica executable from the
[latest-dev report](performance-dev-20260908.md) and the same pinned HyperFLINT
reference. All 33 attachment rows are retained. **The capped run was interrupted to raise the hard-case budgets.**

3 cases have complete, exactly validated paired timings; 3 direct
cases have recorded failed comparisons or reference probes in this snapshot. The 23 SubTropica pipeline inputs
still require frontend preparation and supply no backend speed ratio. Thus
this is not a completed 33-case end-to-end benchmark.

See the [larger-budget continuation](hf-benchmarks-20260908.md) for the current run.

## Direct inputs

Times below are fresh-process wall-time medians from three adjacent alternating
pairs after one warmup. The ratio is the geometric mean of the three individual
HyperLica/HyperFLINT ratios. Every successful preflight and timed response passes
the existing exact comparison/repeatability checks. A ratio below 1 favors
HyperLica. Failed or incomplete cases have no ratio.

| Case | Status | HyperFLINT s | HyperLica s | Paired ratio | Peak MiB C++ / Rust |
|---|---|---:|---:|---:|---:|
| `tst0` | measured | 0.451 | 0.137 | 0.305 | 18.012 / 22.527 |
| `tst1` | measured | 8.003 | 3.696 | 0.464 | 33.566 / 71.797 |
| `tst2` | measured | 123.741 | 70.686 | 0.565 | 401.137 / 1092.109 |
| `tst3` | preflight_failed | — | — | — | — / — |
| `tst4` | interrupted_for_larger_budgets | — | — | — | — / — |
| `findroots21_a` | awaiting_candidate | — | — | — | — / — |
| `findroots21_b` | awaiting_candidate | — | — | — | — / — |
| `parity_face` | awaiting_candidate | — | — | — | — / — |
| `qbox_one_mass` | reference_failed; awaiting_candidate | — | — | — | — / — |
| `qbox_collaborator` | reference_failed; awaiting_candidate | — | — | — | — / — |

## Heavy-case preflight outcomes

These are single execution probes, including failed attempts, rather than paired
performance samples. An allocation failure under RLIMIT_AS does not prove that
the host ran out of physical memory. Timeout durations are not completed timings.

| Case | Backend | Outcome | Elapsed s | Peak MiB | Timeout s / address-space MiB |
|---|---|---|---:|---:|---|
| `tst2` | HyperFLINT | ok | 126.152 | 400.47 | 900 / 8192 |
| `tst2` | HyperLica | ok | 70.169 | 1094.01 | 900 / 8192 |
| `tst3` | HyperFLINT | timeout | 1800.032 | 7522.31 | 1800 / 8192 |
| `tst3` | HyperLica | memory_limit_or_allocation_failure | 658.471 | 8064.34 | 1800 / 8192 |
| `parity_face` | HyperFLINT | ok | 24.967 | 59.14 | 600 / 8192 |
| `qbox_one_mass` | HyperFLINT | memory_limit_or_allocation_failure | 228.969 | 6030.06 | 600 / 8192 |
| `qbox_collaborator` | HyperFLINT | memory_limit_or_allocation_failure | 205.759 | 6046.99 | 600 / 8192 |

HyperFLINT `tst4` exited on SIGSEGV after 0.171 seconds. A separate
bounded rerun on CPU194 reproduced SIGSEGV after 0.227 seconds; neither
attempt produced output. Both used the inherited 8MiB stack limit. Raising
only the stack limit to 64MiB allowed a separate C++ probe to run until its
10-second timeout (116.12MiB peak RSS), without the immediate SIGSEGV.
This suggests stack exhaustion in the default configuration; the short
raised-stack probe is not a completed calculation or a performance sample.
Both C++ quadruple boxes reported a failed 512MiB
allocation under the configured 8GiB address-space limit.

## Pipeline inputs

The following 23 rows require the same projective gauge, analytic continuation
and subtraction terms, and requested epsilon order before either backend runs.
No prepared backend requests were supplied. The available SubTropica source
checkouts do not supply a runnable Wolfram kernel or the missing configuration.
Raw epsilon expansion is not an equivalent replacement. The literal-zero
triangle also requires defined frontend handling of unused variables.

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

## Method and provenance

- HyperLica binary SHA-256: `cf72945dcada1632e31dca72e883ad91997d741e755b8050af335a16963ffea4`.
- HyperFLINT binary SHA-256: `8140b11d4628defa83301a1e37790b1af78a7bce1da3dc0d5a17ec17c723d8b1`.
- Original attachment SHA-256 recorded by the prior run: `909be0ef1bc030a00ea823190733329c228a183e8f8e55ff909c56000c461e3c`.
- The original attachment directory is unreadable by this session. Requests are
  replayed from the prior resolved inventory; all ten request hashes and the
  measured executable, runtime and comparison-harness hashes are verified.
  This run does not claim to have revalidated the original attachment bytes.
- All warmups and timed pairs use CPU196, one thread, on the shared AMD EPYC9754
  host. CPU196 has a singleton SMT sibling list. Affinity is not a reservation.
- Reference-only preflights run on CPU195 and are reused solely for correctness
  preflight. They may run concurrently with the paired suite on CPU196.
  Every actual timed pair uses fresh invocations of both backends on CPU196.
- Per-process timeout and address-space bounds match the checked-in matrix:
  light cases 60–180 seconds / 4GiB; heavy cases 600–1800 seconds / 8GiB.
  Both backends inherit an 8MiB stack limit in the main run. Output files are
  capped at 64MiB, core dumps are disabled, and timeout kills
  the process group. Startup, parsing and serialization are included in timings.
- The supplied license is loaded privately and its value is not recorded.
  The aborted initial `paired/` launch lacked the environment variable and is
  excluded; the valid continuation is `paired-licensed/`.

This is exploratory evidence; three observed pairs are not confidence intervals
or a universal runtime/memory parity qualification. The report preserves failed
and unavailable cases rather than incorporating them into speed aggregates.

## Evidence and reproduction

Local artifacts are under `target/hf-all-20260908/`:

- `paired-licensed/{results.json,summary.csv,overview.md,inventory.json,raw/}`:
  complete attachment inventory, per-case results, samples and raw outputs.
- `reference-preflight/`: the separate bounded C++ execution probes.
- `README.md`, `run.py`, and `run-reference-original.py`: invocation and source
  provenance, including an immutable copy of the original reference driver.
- `tst4-crash-recheck/` and `tst4-stack-probe/`: the default-stack crash
  reproduction and separate, 10-second raised-stack diagnostic.
- `tst3-rust-resource-trace.csv` and `tst4-rust-resource-trace.csv`: lightweight
  diagnostic samples of process address space/RSS; official peaks and times
  still come from the process measurement helper.
- `report.py`: generation of this report from the saved evidence.

The raw artifacts are ignored local files, not included in a fresh clone.
The existing [matrix instructions](performance-matrix.md) reproduce the direct
selection when the original attachment and executables are available:

```bash
python3 scripts/performance_matrix.py \
  --reference /absolute/path/to/hyperflint-cpp \
  --candidate /absolute/path/to/hyperlica \
  --attachment /absolute/path/to/hf_benchmarks.json \
  --case 'attachment.*' --pairs 3 --warmup 1 --cpu 196 \
  --output target/attachment-new-run
```

Supply `SYMBOLICA_LICENSE` through the parent environment. This command freshly
runs both preflights; pipeline rows remain unavailable until frontend requests
and preparation choices are supplied.
