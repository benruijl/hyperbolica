# Implementation cross-audit, 2026-09-07

Three independent reviewers covered period reduction/output, integration and
fibration, and polynomial/rational arithmetic. Each then reviewed another
reviewer's changes. The coordinating review covered input lowering, benchmark
provenance, symbol lookup, and end-to-end compatibility.

## Findings and changes

| Area | Change and correctness guard |
|---|---|
| Period representation | Evaluate numeric boundary words in the ten-variable standard MZV basis; keep MZV powers outside the kinematic polynomial ring. Explicit input constants, custom tables, and algebraic-letter requests retain the existing basis context. |
| Period evaluation | Reuse the validated eager standard expansion and a bounded cache of context-independent numeric period decompositions. Cache entry count, word length, and term count are limited. |
| Period identity/output | Use the same canonical IDs for minted MZVs and Pi² reduction. Native output reconstructs registered MZV atoms; the JSON bridge emits legacy MZV names. Arbitrary registered period keys remain opaque. |
| Input lowering | Detect indexed constants before choosing a slim context. Deduplicate equivalent registered aliases such as `MZV[3]` and `mzv_3`, preserving the first spelling and distinct namespaces. |
| Rational arithmetic | Check shared variable-map identity before structural equality; construct scalars directly over integers; share zero/one arithmetic results and skip substitutions in absent variables. Context/error validation remains active. |
| Partial fractions | Factor the integer denominator directly and project numerator coefficients once for all poles. Reconstruction tests cover scalar content, symbolic leading coefficients, repeated poles, and algebraic roots. |
| Integration | Share recursive word transforms within a step, discard caches before primitive evaluation, and avoid repeated regulator canonicalization/hashing and identity shuffles. |
| Fibration | Propagate the reduction table through recursive transforms, matching the C++ implementation. |
| Algebraic registry | Hold a re-entrant registry session across cached low-level operations so another request cannot reset IDs while a cache is live. |
| Shared lookup | Cache registered Symbolica heads and the standard reduction-rule index; scan the context instead of recreating all 700 rule LHS atoms on each call. |
| Profiling example | Exclude compute-time telemetry from repeatability checks while retaining exact result comparisons and checking failure envelopes. |

The initial CPU-pinned prototype profile contained 5,152 userspace samples with
no lost samples. Variable equality and memory comparisons accounted for about
25% of self samples; repeated symbol lookup was another visible cost. These
findings motivated the shared-context and registered-head changes. The profile
is diagnostic, not a benchmark measurement.

## Verification

- 420 library tests passed, including 86 exact wide-versus-tuple boundary-word
  comparisons, Pi² cancellation, native/legacy output, negative powers, and
  context/cache isolation.
- Integration, CLI, C ABI, Smirnov `tst0`, and benchmark smoke tests passed via
  `cargo test --all-targets`.
- All 47 benchmark-evidence tests passed.
- All 79 C++ differential cases passed (24 use their documented normalized
  comparison rather than byte equality), in both debug and the final release
  build.
- All 18 benchmark preflights passed. Fresh `findroots21_a` and `findroots21_b`
  root definitions were also checked independently: root index, polynomial,
  discriminant, sum/product, variable identity, and root symbols agree.
- The 600-line Rust module-size gate, formatting, and whitespace checks passed.

Final differential and timing evidence is recorded under
`target/cross-audit-20260907/`.

## Timing overview

Rust has lower medians on 16 of 18 selected workloads. Repeated partial
fractions are approximately tied by paired ratio. `tst1` remains substantially
slower and more memory-hungry than C++; overall performance parity is **not**
established.

All times below are cold-process wall-time medians in milliseconds. Speedup is
C++ median / Rust median; a value below one means Rust is slower. RSS is the
maximum observed peak across the five measurements, in MiB.

| Workload | C++ ms | Rust ms | Rust speedup | Paired Rust/C++ ratio | Peak RSS C++ / Rust |
|---|---:|---:|---:|---:|---:|
| `tst0` | 442.075 | 419.847 | 1.05× | 0.950 | 18.0 / 42.5 |
| `tst1` | 9,686.570 | 28,172.984 | 0.34× | 2.539 | 36.6 / 109.6 |
| `findroots21_a` | 113.446 | 12.255 | 9.26× | 0.103 | 27.1 / 14.5 |
| `findroots21_b` | 38.156 | 7.694 | 4.96× | 0.199 | 27.2 / 14.5 |
| `convergent_integration_step` | 25.684 | 5.322 | 4.83× | 0.203 | 19.3 / 12.1 |
| `dense_parameter_resultant` | 1,959.286 | 326.117 | 6.01× | 0.165 | 43.6 / 67.5 |
| `dense_polynomial_multiply` | 10.796 | 4.810 | 2.24× | 0.447 | 12.1 / 12.1 |
| `euler_filtered_massless_box` | 12.681 | 5.624 | 2.25× | 0.442 | 12.1 / 12.1 |
| `factor_table_multistage` | 11.524 | 5.557 | 2.07× | 0.483 | 12.1 / 12.1 |
| `high_order_laurent_series` | 14.292 | 7.850 | 1.82× | 0.452 | 12.1 / 12.1 |
| `large_multiplicity_partial_fractions` | 52.633 | 55.288 | 0.95× | 1.002 | 27.1 / 22.6 |
| `lr_verify_three_variables` | 13.032 | 6.255 | 2.08× | 0.488 | 12.1 / 12.1 |
| `multivariate_gcd` | 18.123 | 7.961 | 2.28× | 0.454 | 12.1 / 12.1 |
| `production_mzv_reduction` | 18.018 | 6.799 | 2.65× | 0.386 | 15.1 / 14.5 |
| `shared_denominator_rat_sum` | 14.646 | 10.945 | 1.34× | 0.653 | 12.1 / 14.5 |
| `sparse_high_degree_multiply` | 14.417 | 6.727 | 2.14× | 0.467 | 12.1 / 12.1 |
| `three_variable_hyperflint` | 27.862 | 8.533 | 3.27× | 0.311 | 15.1 / 12.1 |
| `tiny_rational_addition` | 16.288 | 8.012 | 2.03× | 0.468 | 12.1 / 12.1 |

The 14 smaller workloads have a workload-unweighted geometric mean paired
Rust/C++ ratio of 0.421. Across all 18 it is 0.432. These averages give a tiny
kernel the same weight as `tst1`: the sum of the four attachment medians is
28.613 seconds for Rust versus 10.280 seconds for C++. That is a 2.783× longer
observed batch, despite three of its four individual cases favoring Rust.

### Historical comparison and uncertainty

Compared with the earlier validated attachment run:

- `tst0`: Rust median 1.012 s → 0.420 s (58.5% lower); peak RSS 50.6 → 42.5 MiB.
- `tst1`: Rust median 30.689 s → 28.173 s (8.2% lower); peak RSS 142.3 →
  109.6 MiB (23.0% lower).
- `findroots21_a`: Rust median 16.555 ms → 12.255 ms.
- `findroots21_b`: Rust median 7.889 ms → 7.694 ms.

These are historical observations, not a contemporaneous old/new code A/B
test. The machine is shared and startup adaptation differs from the historical
run. In particular, current `tst1` samples range from 16.017–29.260 s for Rust
and 7.846–13.983 s for C++. Its paired ratios range from approximately
2.04–2.99; their geometric mean is 2.539, with a one-sided 95% bootstrap upper
bound of 2.898. The 8.2% historical median reduction should not be interpreted
as a precise or statistically established speedup. The memory reduction is
much more consistent across the observed runs.

`tst2`, parity-face, both quadruple boxes, `tst3`, and `tst4` were not rerun in
this audit. These 18 workloads include only four of the 33 attachment entries;
they do not cover the SubTropica graph-subtraction/epsilon-expansion pipeline.
Prior limitations on those inputs are not resolved by this benchmark.

### Remaining hotspot

A separate final `tst1` profile collected 4,799 userspace CPU samples with no
loss. Integer GCD accounts for about 19.9% of self samples, packed polynomial
division for 16.3%, and integer multiplication for 10.3%. The earlier dominant
variable-equality and registered-head lookup costs no longer appear above the
1% report threshold. This supports the targeted overhead reductions, but does
not attribute all remaining arithmetic time to any one integration operation.
Reducing exact-arithmetic work remains the main target for closing the `tst1`
gap; these profiling samples are excluded from the timing table.

## Benchmark provenance

The Rust build uses the checked-in release profile (LTO, one codegen unit) and
the unmodified local Symbolica revision
`0b57776bf911faeea7e28ea133706fb03740ffeb`.
The final Rust executable SHA-256 is
`af839f2a714bfb3ec00297fe0e67f81a1ff0cf1ef8b27a0b93f8f164bd23e7c7`.

The C++ reference is the same release-portable, mimalloc-enabled HyperFLINT
executable used in the earlier attachment benchmark:
`/tmp/hyperflint-attachment-release-portable/hyperflint`, SHA-256
`8140b11d4628defa83301a1e37790b1af78a7bce1da3dc0d5a17ec17c723d8b1`,
source revision `adfd3af3be234cb43a2322bd9ec442caa26edd74`.
Its original Nix FLINT library path is no longer installed. The audit's small
launcher supplies the available FLINT 3.6.0 library, SHA-256
`554c25b57892dcf15efd92e0e640c37457a96b711a39d96d42d51752221bccf2`.
This runtime-library change is why historical before/after ratios are described
separately from the fresh paired C++/Rust comparison.
Both measured commands use matching Bash `exec` launchers so the C++ runtime
adapter does not impose unmatched shell startup overhead. Artifact metadata
hashes these launchers; the underlying executable hashes are recorded here.

Measurements use CPU 36, one algebra thread, fresh processes, monotonic wall
time, and `wait4` peak RSS. Compilation and result canonicalization are excluded
from timed intervals; launcher/process startup, input parsing, output
serialization, and teardown are included. The
benchmark script checks complete response parity under each fixture's existing
comparison policy before collecting pairs. These are exploratory measurements
from a dirty worktree, not a clean-release qualification claim.
Five pairs with one warmup per backend support a coarse timing overview, not a
precise guarantee. The reported speedup from medians is C++ median divided by
Rust median; the raw analysis separately records geometric means of paired
Rust/C++ ratios and their confidence intervals. Relaxed collection thresholds
are not evidence that every workload meets the repository's qualification gate.

## Evidence and reproduction

The following artifacts are retained in `target/cross-audit-20260907/`:

- `paired/analysis.json`, `paired/summary.csv`, and `paired/samples.csv`: complete
  statistics and all 90 measured pairs.
- `paired/correctness.json` and `paired/metadata.json`: comparison hashes,
  measurement settings, executable adapters, and build provenance.
- `case{6,7}-{cpp,rust}.algebraic-common.json`: independently checked algebraic
  definitions, all with SHA-256
  `eced5ae6dfbccf600f992c09ce4740c92ce53d7b10322ffc5a361cdd1543a814`.
- `tests-final.log`, `differential-release.log`, and `evidence-tests.log`:
  successful test runs.
- `final-perf.data`, `final-profile-report.txt`, and `final-profile-response.json`:
  diagnostic profile, not timing evidence.

With the adapters above available, the existing release executable built, and
`SYMBOLICA_LICENSE` set in the environment, the collection command is:

```sh
BUILD_RUST=0 BUILD_CPP=0 \
HYPERFLINT_RUST="$PWD/target/cross-audit-20260907/hyperbolica-rust" \
HYPERFLINT_CPP="$PWD/target/cross-audit-20260907/hyperflint-cpp" \
HYPERFLINT_CPP_SOURCE=/tmp/hyperflint-inspect.Fl8ksV/SubTropica/HyperFLINT \
HYPERFLINT_CPP_CMAKE_CACHE=/tmp/hyperflint-attachment-release-portable/CMakeCache.txt \
BENCH_WORKLOADS="$PWD/target/cross-audit-20260907/workloads.jsonl" \
BENCHMARK_MODE=exploratory BENCHMARK_TIER=all \
PAIRS=5 WARMUP=1 THREADS=1 CPUSET=36 BOOTSTRAP_SAMPLES=10000 \
GLOBAL_UPPER_CI=100 MAX_WORKLOAD_RATIO=100 MAX_RSS_RATIO=100 \
EVIDENCE_DIR="$PWD/target/cross-audit-20260907/rerun" \
bash scripts/benchmark-compare.sh
```
