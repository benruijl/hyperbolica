# Performance parity work: measured progress, 2026-09-07

Status updated 2026-09-08. This report supersedes the timing conclusions, but
not the historical evidence, in `performance-audit-20260907.md`. Runtime parity
across every input and memory parity have **not** been established. The combined
stage-4 implementation has completed correctness checks and a release build,
but has no completed end-to-end performance comparison in this report.

## Completed candidate 1 measurements

Candidate 1 uses the unmodified vendored Symbolica dev revision
`0b57776bf911faeea7e28ea133706fb03740ffeb`. Its principal changes are balanced
canonical rational accumulation during integration, consuming work queues,
integer-coefficient multiplication/materialization, and fewer redundant
rational normalizations. These changes replace repeated expanded/factored
round trips identified in the integration profile.

The full 158-case core/API/scaling matrix passes exact comparison under each
fixture's documented policy. The candidate has a paired runtime ratio below
one in 148 cases. These are exploratory observations, not a statistical
qualification or a guarantee for other polynomial shapes.

| Group | Measured | Paired ratio below 1 | Geometric mean Rust/C++ ratio |
|---|---:|---:|---:|
| Locked benchmark workloads | 14 | 14 | 0.446 |
| Differential/API workloads | 79 | 77 | 0.491 |
| Generated scaling workloads | 65 | 57 | 0.582 |

Small API cases include process startup; they do not establish a corresponding
speedup for an individual arithmetic primitive. Group averages weight every
workload equally, regardless of its running time.

### Supplied attachment: four completed light cases

Five adjacent C++/Rust pairs, with one warmup per backend. Times are fresh-process
wall-time medians, in milliseconds. The paired ratio is the geometric mean of
the five Rust/C++ sample ratios, **not** the ratio of the two medians.

| Case | C++ ms | Rust ms | Paired Rust/C++ ratio | Peak MiB C++ / Rust |
|---|---:|---:|---:|---:|
| `tst0` | 811.624 | 299.007 | 0.371 | 18.06 / 22.52 |
| `tst1` | 12,457.470 | 3,852.943 | 0.384 | 35.86 / 73.39 |
| `findroots21_a` | 115.242 | 12.834 | 0.111 | 27.19 / 14.50 |
| `findroots21_b` | 38.523 | 8.441 | 0.216 | 27.19 / 14.49 |

`tst1` is faster in all five pairs (Rust/C++ ratios 0.243–0.486), but still
uses approximately twice the peak memory. Its historical 28.173-second Rust
median is not a contemporaneous baseline/candidate A/B measurement and should
not be used to claim a precise code-only speedup.

### Remaining candidate 1 runtime gaps

| Case | C++ ms | Rust ms | Paired Rust/C++ ratio |
|---|---:|---:|---:|
| `api.dispatch_zero_inf_period` | 28.610 | 76.074 | 2.603 |
| `api.dispatch_zero_one_period` | 27.931 | 79.354 | 2.709 |
| `scale.dense_power.d12.v3` | 15.696 | 15.783 | 1.016 |
| `scale.partial_fractions.p4.m6` | 92.221 | 187.116 | 2.035 |
| `scale.period.zero_one.w2` | 30.664 | 80.333 | 2.561 |
| `scale.period.zero_inf.w2` | 30.927 | 75.808 | 2.431 |
| `scale.period.zero_one.w4` | 34.419 | 78.556 | 2.238 |
| `scale.period.zero_inf.w4` | 40.124 | 78.366 | 1.952 |
| `scale.period.zero_one.w6` | 35.921 | 78.405 | 2.130 |
| `scale.period.zero_one.w8` | 40.484 | 75.647 | 1.883 |

The 1.6% dense-square gap is small; it is not treated as a robust regression.
Period startup and repeated-pole partial fractions are substantive gaps.

### First heavy probe

`tst2` passed exact C++/Rust comparison. One unprofiled adjacent pair measured
189.678 s for C++ and 93.565 s for Rust (ratio 0.493). Peak RSS was 397.71 MiB
and 1,092.26 MiB respectively. This is a promising runtime probe, not a repeated
estimate; memory is still substantially worse. A separate profile attached
only to the initial correctness preflight is explicitly excluded from timing
evidence in `candidate1-heavy-probe/PROFILING-NOTE.md`.

The parity-face preflight found a correctness gap in both candidates 1 and 2:
C++ returns eleven terms, while Rust rejects an intermediate rational function
with a context-map error. Neither run has a performance ratio. Subsequent native
and adapter fixes pass focused context regressions, but the complete parity-face
comparison remains unverified; lower rejection time must not be called a speedup.
Exact raw responses are retained in `candidate1-heavy-probe` and
`candidate2-parity-face`.

## Candidate 2: completed 162-case matrix

- Lazy initialization of the standard 700-rule MZV expansion table, preserving
  explicit/custom/disabled expansion behavior and validating all lazy rules
  against the eager oracle.
- Proper linear factored partial fractions computed without first expanding
  powered denominators; unsupported shapes retain their previous fallback.
- Factored JSON input lowering for bare rational integrations and partial
  fractions, with exact CLI and large-power structural regressions.

All 443 library tests, integration/CLI/ABI tests and all-target smoke tests
passed. The 79 debug and release C++ differential cases passed; formatting passed. The
release executable is preserved separately before linking modified Symbolica.

All 162 core/API/scaling/light-attachment preflights and repeatability checks
passed. Of their three-pair runtime ratios, 159 favor Rust. The remaining
results are a dense square at 1.011, three sixth-order poles at 1.021, and four
sixth-order poles at 2.005. The first two are near-ties; the four-pole case is a
clear remaining performance gap. The separately probed parity-face rejection
is not included among these successful calculations.

| Group | Measured | Paired ratio below 1 | Geometric mean Rust/C++ ratio |
|---|---:|---:|---:|
| Locked benchmark workloads | 14 | 14 | 0.440 |
| Differential/API workloads | 79 | 79 | 0.469 |
| Generated scaling workloads | 65 | 62 | 0.470 |
| Light attachment cases | 4 | 4 | 0.218 |

All endpoint-period cases now favor Rust. For example, `zero_one` at weight 4
is 11.175 ms versus C++ 35.243 ms; `zero_inf` at weight 8 is 35.096 ms versus
317.882 ms. These are current paired C++ comparisons, not direct candidate
1/2 code-only A/B estimates.

| Attachment case | C++ ms | Rust ms | Paired Rust/C++ ratio | Peak MiB C++ / Rust |
|---|---:|---:|---:|---:|
| `tst0` | 810.770 | 224.308 | 0.274 | 18.07 / 22.50 |
| `tst1` | 15,628.159 | 4,709.867 | 0.364 | 33.60 / 71.35 |
| `findroots21_a` | 197.584 | 21.336 | 0.107 | 27.19 / 14.52 |
| `findroots21_b` | 65.221 | 13.974 | 0.210 | 27.18 / 14.48 |

The timing drift between candidate 1 and 2 affects both backends, particularly
the later attachment samples. Do not infer a Rust regression merely by
comparing its 3.853-second historical candidate 1 `tst1` median with the
4.710-second candidate 2 median: a dedicated adjacent old/new comparison is
needed to isolate code changes.

The four-pole gap is still 189.090 ms versus C++ 94.635 ms. Its repeated-call
profile attributes about 70% inclusive time to native multivariate GCD during
coefficient multiplication. C++ keeps the intermediate recurrence coefficients
factored; the candidate-2 Rust path uses canonical expanded rational functions.
Avoiding powered input-denominator expansion alone therefore does not remove
the repeated coefficient-normalization cost. The later stage-4 implementation
moves this recurrence into a generic factored-coefficient Symbolica API.

Candidate 3 was built against `target/symbolica-stage1-audit`, an immutable
copy containing only the validated fraction/GCD patch. Its release build and
all-targets tests passed, and the executable is archived. The adjacent
candidate-3/candidate-2 comparison passed all 162 exact checks. Runtime effects
are mostly small: group geometric means are 0.997 for API, 0.987 for core and
0.985 for scaling, with 88/162 ratios below one. The attachment group was 1.039,
including one late `tst1` sample substantially slower than its other samples.
These measurements do not demonstrate a broad end-to-end improvement from
the scalar patch alone.

A seven-pair follow-up retains all samples and gives the following direct
old-Rust/new-Rust results, **not** C++ comparisons:

| Workload | Candidate 2 ms | Candidate 3 ms | Paired new/old ratio |
|---|---:|---:|---:|
| Empty algebraic-letter control | 5.055 | 5.096 | 1.019 |
| Dense three-variable degree-12 power | 16.161 | 9.276 | 0.580 |
| Four sixth-order poles | 110.237 | 110.297 | 1.000 |
| `tst1` | 5,971.800 | 5,950.010 | 0.968 |

The dense-power gain is clear; `tst1` remains close to unchanged and the
repeated-pole bottleneck is not addressed by this scalar-only patch. Evidence
directories are `candidate3-stage1-vs-candidate2` and
`candidate3-stage1-targeted-repeat`.

Candidate 4 was built and tested against the immutable
`target/symbolica-stage4-native-audit` snapshot. It includes the bulk polynomial
work, native factored-coefficient partial fractions, rational arithmetic
shortcuts and the constructor/context fixes. The port delegates its proper
linear recurrence and shared-denominator/parameter-derivative shortcuts to
Symbolica. Its context boundary remaps legitimate native variable subsets and
permutations structurally, while rejecting unknown active or unused declarations.
The two CLI regressions for sparse pole orders and reordered contexts pass.
The earlier stage-3 all-library run passed 448 tests and exposed one native
empty-source-map panic; the stage-4 dependency includes its upstream fix.
Candidate-4 all-targets testing passed, including all 449 root library tests
and the integration, CLI, ABI and example targets. The 79 debug C++ differential
cases passed under their documented comparison policies. The release build
completed and its executable is archived; no release differential result or
combined stage-4 end-to-end timing is claimed here. All 47 benchmark-evidence
and harness tests passed after correcting the test subprocesses' Python PATH.
The parity-face case still needs a complete new end-to-end exact comparison.

The frozen stage-4 native Symbolica full-library run passed 625 of 627 tests.
Both failures are reproduced on pre-audit vendor artifacts: the root-isolation
test's initial interval endpoint disagreement (refined intervals match), and
the randomized `galois_upgrade` factorization's non-monic-divisor assertion in
`quot_rem_univariate_monic`. Neither failure is silently counted as passing.

A later native constructor fix merges identical denominator bases after
sign/monic normalization even when factorization is disabled. Its focused suite
passed 21/21 tests, including exact arithmetic and finite/algebraic-field cases.
This later fix is **not** in the immutable stage-4 source, release executable,
449-test port result, or 627-test native run above; those results must not be
attributed to a combined build containing it.

### 2026-09-08 commit and vendor handoff

The generic changes and portable MREs are committed as three local Symbolica
commits through `e33a1fb70abe70e774dd6a55baa5a8e1f42cc8e3`. The clean active
vendor checkout has been fast-forwarded to that tip, and Cargo again uses
`vendor/symbolica`. The complete mail patch is tracked in the parent repository;
an independent application to the original base reproduced the exact pinned
tree. See `vendor/SYMBOLICA.md` for identities and reproduction commands.

A final combined native full-library rerun passed 629/630 tests. The interval
expectation remains a deterministic failure. The intermittent factorization
test passed that suite run, but its standalone MRE still failed on patched
source, so it remains unresolved. An initial rerun without a compiler on PATH
also failed a native-code-generation test; correcting PATH removed that
environment-only failure. The fixed constructor/context/PF MREs pass on
patched source and fail against pre-audit vendor artifacts.

The rebuilt current vendored port passed `cargo test --locked --all-targets`,
including all 449 library tests, and all 79 debug C++ differential cases under
their documented comparison policies. The 47 benchmark-evidence tests and
15 performance-matrix tests pass. Formatting, module-size, shell syntax, and
the updated pure-Symbolica source/dependency gate also pass. The final logs are
`tests-vendored-commit-final.log`,
`differential-vendored-commit-configured-oracle.log`,
`evidence-tests-vendored-commit-clean-env.log`, and
`matrix-tests-vendored-commit.log` in the audit evidence directory. The C++ run
uses the existing pinned executable/runtime adapter; an initial invocation
without its shared-library path was an environment failure, not a comparison.

This packaging operation does not supply new end-to-end timings. Historical
raw evidence under `target/` is local and is not included in a fresh checkout.

### Completed `tst3` and `tst4` bounded probes

The candidate-3/stage-1 probes completed unsuccessfully, using a 1,800-second
timeout and an 8 GiB address-space bound per process. No case reached a successful
exact comparison, and neither has a performance ratio.

| Case | C++ outcome | Rust outcome |
|---|---|---|
| `tst3` | Timeout after 1,800.555 s | Allocation failure after 831.327 s |
| `tst4` | SIGSEGV after 0.192 s | Timeout after 1,800.320 s |

These are resource-limited or failed preflights, not successful integration
times, and they do not test the later stage-4 implementation. Full statuses,
peak memory and raw output are retained in `candidate3-tst34-probes`.

### Quadruple-box allocation failures

Both supplied quadruple-box cases failed their candidate 2 preflights under
the 8 GiB per-process address-space bound. C++ printed FLINT allocation
exceptions on stdout; Rust reported allocation failure on stderr. Neither
case has successful exact output or a performance ratio. These are failures
of both bounded runs, not evidence of faster integration by the earlier abort.

| Case | C++ failure after | Rust failure after |
|---|---:|---:|
| `qbox_one_mass` | 348.282 s | 13.433 s |
| `qbox_collaborator` | 295.754 s | 21.471 s |

Raw evidence and peak memory are in `candidate2-qbox-probes`. Its original
classifier labels C++ as `process_error` because it only inspected stderr;
retained stdout identifies the allocation exception. The harness now also
inspects bounded stdout diagnostics, with a regression test, without changing
the old results.

## Upstream-first arithmetic plan

The separately cloned official Symbolica dev head matched the original vendored
revision above. Switching checkouts alone therefore supplied no new code.
The local branch `codex/hyperlica-rational-fastpaths` is in
`target/symbolica-dev-audit`.

Generic improvements belong in Symbolica/Numerica, not additional port-only
arithmetic wrappers. Existing wrapper workarounds are migration candidates once
equivalent native dispatch is validated. Integration algorithms and embedded
MZV policy remain in the port. Upstream changes will be reviewed and benchmarked
before preparing the requested PR with Ben Ruijl as reviewer; no PR has been
opened yet.

The first frozen upstream patch provides denominator-one fraction arithmetic,
fused in-place integer-backed fraction operations, and unsigned-magnitude
Euclidean GCD. All 144 Numerica tests passed. Standalone paired coefficient
benchmarks show substantial gains for denominator-one convolutions and little
change for genuine rational coefficients; these are not full-port speedups.

Native polynomial diagnostics identified a generic target: Q-polynomials
with integral coefficients missed Z-polynomial bulk kernels. One baseline
three-variable degree-20 by degree-19 multiplication measured approximately
184 ms over Q versus 2.6 ms over Z, and exact division approximately 194 ms
versus 48 ms. These diagnostic observations motivate generic multiplication
and exact-division dispatch work; they do not establish a 72-fold end-to-end
gain or parity with FLINT. The guarded bulk dispatch is included in stage 4.

The combined bulk-arithmetic patch passed all 150 Numerica GMP/MPFR tests and
all 127 alternate-backend Malachite/Astro tests. Guarded fraction kernels avoid
lifting sparse or denominator-diverse inputs when conversion is unlikely to
pay off. Matched standalone native benchmarks use identical dependency locks,
features, release profiles and archived build directories; older native
timings with mismatched build profiles remain diagnostic only.

The cross-audit also found generic Symbolica correctness issues: powered
denominator cancellation, sign/leading-unit normalization at repeated powers,
Q-to-Z scalar conversion of separately powered factors, zero p-adic terms,
and empty-variable polynomial rearrangement. Their fixes and regression tests
are included in the frozen stage-4 snapshot. The subsequent duplicate-base
constructor fix and the two baseline-reproduced native test failures are
reported separately above; stage-4 validation does not cover later edits.

A matched native partial-fraction microbenchmark (same Symbolica library for
both coefficient representations) measures four sixth-order poles at 32.042 ms
with factored coefficients versus 173.517 ms with canonical rational
coefficients, including final coefficient expansion in the timed region;
exact coefficient comparison is checked outside it. This is a 5.42-fold representation-level improvement, not yet a
combined-patch or full-port timing. Simple-pole inputs are slower by tens of
microseconds, so their tradeoff remains under audit. Source, all nine shapes
and measurements are in `factored-pf-native-probe-monic.csv` and the upstream
`examples/factored_partial_fractions.rs` example.

## Coverage and evidence

The inventory contains 191 rows: 14 locked workloads, 79 API cases, 65 generated
scaling cases, ten direct attachment integrations, and 23 attachment inputs
requiring SubTropica preprocessing. The latter need projective gauge selection
and divergent-facet analytic continuation/subtractions before epsilon series.
Expanding their raw integrands in epsilon is not equivalent to that pipeline.
Those unavailable rows remain visible and are not reported as successes.

Complete per-case times, memory, sample ranges, exact comparisons, and hashes:

- `target/parity-audit-20260907/candidate1-matrix/overview.md`: all 191 rows,
  including the 158 measured workloads and every unmeasured status.
- `target/parity-audit-20260907/candidate1-matrix/results.json` and `summary.csv`:
  machine-readable results and all raw sample references.
- `target/parity-audit-20260907/candidate1-attachment/`: five-pair evidence for
  all four light attachment cases.
- `target/parity-audit-20260907/candidate2-matrix/`: complete three-pair timing
  overview, CSV, exact comparisons and raw samples for the newer 162-case run.
- `target/parity-audit-20260907/baseline-matrix-v2/`: valid immutable-baseline
  comparison, including corrected independent-child RSS measurement.
- `target/parity-audit-20260907/candidate1-heavy-probe/`: completed `tst2` probe
  and failed parity-face preflight; only the successful exact pair has a ratio.
- `target/parity-audit-20260907/candidate3-tst34-probes/`: completed unsuccessful
  preflights, including each timeout, allocation failure and process error.
- `target/parity-audit-20260907/tests-candidate4-native.log`,
  `differential-candidate4-debug.log` and `build-candidate4-native.log`:
  frozen-stage-4 root validation and completed release-build evidence.
- `target/parity-audit-20260907/factorized-duplicate-arithmetic-tests.log`:
  the later constructor fix's separate 21-test focused validation.
- `target/parity-audit-20260907/mre-galois-vendor-debug.log`: pre-audit vendor
  reproduction of the randomized native factorization assertion.

The earlier `baseline-matrix` directory is superseded: its long-lived measurement
parent contaminated small-child RSS. Its warning file and raw evidence are
retained. Only `baseline-matrix-v2` uses the corrected measurement lifecycle.

Measurements use CPU 36, one algebra thread, matching Bash exec launchers,
fresh measurement helpers, per-case address-space limits, and whole-process-group
timeouts. Compilation and validation are excluded; parsing, process startup,
serialization and teardown are included. The host is shared, so raw adjacent
pairs matter more than historical wall-time comparisons.

Candidate 1 executable SHA-256:
`23eaec48a0dc5f468288e83faa5ad911b73da6f1fdb7d76a03e9ecd80fffcf13`.
Source archive SHA-256:
`a5aa77819fbc88cf224d580791b53f8f1317aacda76faf6710cd852aa68dcf76`.
Candidate 2 source archive SHA-256:
`2d0df9474bcb3035ee5a2a991b1d75443119899af6b894700d013b068348a472`.
Candidate 2 executable SHA-256:
`4200f38f561339855fcea4f4ff8178824642b764aa93683e5919292ae3c73b2d`.
Candidate 3 stage-1 executable SHA-256:
`9138cbc00ebef5cfa05e730e7519dc5ee7abed782723b363742f0a608b02ec41`.
Candidate 3 stage-1 source archive SHA-256:
`3202853e4b0b7e9e0245e6f18c28dafab066a65e0f6e5213f7c034757ce7c6a6`.
Candidate 4 source archive SHA-256:
`bf70b10478c24fb977ca7a438bc5cbe569a3be872407ffc34df2d1dcb7d74fef`.
Candidate 4 executable SHA-256:
`e66843b3bac6cae3ac1e9acb2c415e9cdd4c4ed1ec75c8af48d7bfa96f0feb7a`.
Stage-4 Symbolica source archive SHA-256:
`d5bbc675e670034b0ba5b187e4ef37aa0fed9df78e2bb818e4cb478a0ed56134`.

The C++ executable and its FLINT 3.6 runtime are unchanged from the immediately
preceding audit; every new measurement records their hashes separately from
the launchers. The license is supplied only through the inherited environment;
evidence records its presence, not its value.
