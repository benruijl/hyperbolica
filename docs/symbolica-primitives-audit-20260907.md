# Symbolica primitive audit, 2026-09-07

This is the native-primitive investigation accompanying the Hyperbolica/HyperLica
performance work. These numbers are not the end-to-end integration timings.

## Source and scope

The official `symbolica-dev/symbolica` `dev` branch resolved to
`0b57776bf911faeea7e28ea133706fb03740ffeb`, exactly the original vendored revision. Merely
changing the dependency path therefore adds no upstream improvements. The
separate working clone is `target/symbolica-dev-audit`; immutable baseline
artifacts remain available for comparison after the patched source is moved
into the vendor checkout (see `vendor/SYMBOLICA.md`). `lib/numerica` is a tracked directory of that
repository, not a submodule. The official commit page identifies the intended
reviewer as [@benruijl](https://github.com/symbolica-dev/symbolica/commit/0b57776bf911faeea7e28ea133706fb03740ffeb).

The scalar/GCD-only patch is independently preserved in
`target/symbolica-stage1-audit`, with patch SHA-256
`6e88a68a2d69c01a48a93ff5fef3f5169c43925c70010b7401b771ebc689f036`.
The patch and a three-file source archive are under
`target/parity-audit-20260907/symbolica-stage1-fraction-gcd.*`.
No license value is written into these artifacts.

## Findings and implemented upstream work

The earlier integration callgraph attributed 37% inclusive time to dense
rational-polynomial multiplication and 15% self time to integer GCD. This was a
profile of the pre-optimization integration path, not a claim about the current
port after its algorithmic improvements.

The native experiments identify distinct issues:

1. `FractionField` used temporary rational products and GCD normalization even
   for denominator-one coefficient arithmetic. The stage-1 patch dispatches
   these operations directly to the underlying ring, including genuinely fused
   add/subtract-product updates.
2. Signed Euclidean GCD used signed remainder at every iteration. Computing on
   unsigned magnitudes preserves signed-minimum boundary behavior and avoids
   signed remainder overhead. A universal binary-GCD replacement was rejected:
   it regressed small-denominator and unbalanced inputs substantially.
3. `Q` did not expose the integer domain's polynomial kernels. The stage-2
   fraction adapter clears a common denominator once per input, delegates
   dense/chunked/total-degree multiplication, then normalizes once per output
   coefficient. It declines fewer than 64 coefficient products or more than 16
   distinct nonunit denominators before doing LCM arithmetic. Nonunit-denominator
   inputs also need at least four coefficient products per dense output cell:
   low-collision inputs otherwise pay for unnecessary denominator growth. The
   variety limit is not an absolute coefficient-bit-size limit. Existing fallback
   arithmetic is retained; malformed basic layouts are rejected before lifting.
4. Dense multiplication has a representation cliff around all-`Double`
   128-bit inputs: they can miss both bounded accumulators and the GMP-array
   path, while larger `Large` coefficients enter fused GMP arithmetic. This
   is addressed in the integer kernel by promoting once per operand, with exact tests.
5. Rational exact division needs coefficient-domain dispatch rather than
   repeated generic fraction normalization. A separate audit implements this
   in Symbolica's `PolynomialGCD::try_div_exact` hook. The native table also
   shows remaining sparse integer-division overhead even after clearing
   coefficient denominators.

The fraction and GCD changes passed 144 Numerica unit tests. With multiplication
adapter tests, its conservative fallback guard, and the separate integer-kernel
regression, the combined Numerica test suite passed 150 tests. Full Symbolica
and full-port validation are additional gates, not implied by that count.
The same frozen stage-2 source also passed all 127 Numerica tests with
`integer-malachite,float-astro` and default features disabled, verifying the
generic changes without the GMP/MPFR backend. Logs are
`numerica-stage2-density-tests-full.log` and
`numerica-stage2-malachite-astro-tests.log` in the audit artifact directory.

## Scalar measurements: every case

Five alternating baseline/candidate process pairs, CPU 56, release optimization,
one thread. Each measured region excludes input construction. Values below are
the median normalized microseconds per outer iteration; add/multiply/fused
iterations each process 48 coefficients, and a convolution is one 48-by-48
coefficient convolution. These compare stage 1 with unchanged upstream, not the
later bulk-polynomial adapter. `few` uses small varying rational denominators;
`one` means integral coefficients represented as fractions.

| Bit parameter | Denominators | Operation | Baseline µs | Stage 1 µs | Speedup |
|---:|---|---|---:|---:|---:|
| 12 | one | add | 1.320 | 0.883 | 1.50× |
| 12 | one | multiply | 2.103 | 0.850 | 2.47× |
| 12 | one | fused round trip | 7.439 | 1.266 | 5.88× |
| 12 | one | convolution | 162.640 | 23.862 | 6.82× |
| 12 | few | add | 3.320 | 3.117 | 1.07× |
| 12 | few | multiply | 2.770 | 2.446 | 1.13× |
| 12 | few | fused round trip | 14.686 | 14.050 | 1.05× |
| 12 | few | convolution | 455.443 | 415.568 | 1.10× |
| 63 | one | add | 1.311 | 0.850 | 1.54× |
| 63 | one | multiply | 2.112 | 0.926 | 2.28× |
| 63 | one | fused round trip | 7.794 | 1.257 | 6.20× |
| 63 | one | convolution | 180.866 | 25.282 | 7.15× |
| 63 | few | add | 3.475 | 3.310 | 1.05× |
| 63 | few | multiply | 2.784 | 2.646 | 1.05× |
| 63 | few | fused round trip | 16.176 | 15.366 | 1.05× |
| 63 | few | convolution | 1252.943 | 1244.798 | 1.01× |
| 127 | one | add | 1.653 | 0.891 | 1.86× |
| 127 | one | multiply | 7.727 | 5.768 | 1.34× |
| 127 | one | fused round trip | 34.098 | 8.720 | 3.91× |
| 127 | one | convolution | 826.008 | 168.550 | 4.90× |
| 127 | few | add | 12.370 | 11.880 | 1.04× |
| 127 | few | multiply | 9.648 | 8.465 | 1.14× |
| 127 | few | fused round trip | 68.743 | 65.171 | 1.05× |
| 127 | few | convolution | 1764.759 | 1708.545 | 1.03× |
| 512 | one | add | 11.810 | 4.353 | 2.71× |
| 512 | one | multiply | 22.034 | 8.923 | 2.47× |
| 512 | one | fused round trip | 92.020 | 15.839 | 5.81× |
| 512 | one | convolution | 1785.702 | 317.877 | 5.62× |
| 512 | few | add | 20.427 | 20.319 | 1.01× |
| 512 | few | multiply | 24.408 | 24.353 | 1.00× |
| 512 | few | fused round trip | 115.907 | 117.025 | 0.99× |
| 512 | few | convolution | 2964.588 | 2946.409 | 1.01× |

Source: `target/symbolica-dev-audit/lib/numerica/examples/rational_fastpaths.rs`.
Raw paired CSVs: `target/parity-audit-20260907/numerica-combined-{1..5}.csv`
and `numerica-combined-baseline-{1..5}.csv`.

## Native polynomial baseline: every operation and coefficient family

These are diagnostic kernel-region averages of four calls, not repeated-pair
medians or a qualification. Both implementations use one thread on CPU 56.
Construction, parsing, and validation are outside the measured region; output
allocation and destruction are included. FLINT is 3.6.0, compiled `-O3 -DNDEBUG`.
Symbolica is the original dev revision, before these primitive patches.

`Q` is the original rational-polynomial path; `Z` performs the mathematically
equivalent operation after exact common-denominator scaling. FLINT uses its
native `fmpq_mpoly` representation, which already separates rational content.
This `Z` column diagnoses dispatch potential: it is not automatically the
achieved performance of the new rational adapter. Every generated product and
exact quotient was cross-checked against FLINT, and Q/Z scalar equivalence was
asserted in Rust.

The dense families contain all monomials of total degree ≤12/11 or ≤20/19 in
three variables: 455×364→2600 and 1771×1540→11480 terms, respectively. The sparse
family has eight variables and degrees 128/127: 17×17→245 terms. `b` is the
generator's bit/shift parameter, not a guaranteed exact bit maximum; the small
dense family can reach 14 bits when `b=12`. `r` denotes varying denominators
from `{1,3,5,7,11}`; `i` denotes integral coefficients.

| Support | b | Coeff. | Operation | Symbolica Q ms | Symbolica Z ms | FLINT Q ms |
|---|---:|---|---|---:|---:|---:|
| dense 3D d12 | 12 | i | multiply | 11.527 | 0.255 | 0.290 |
| dense 3D d12 | 12 | i | exact divide | 16.842 | 4.666 | 2.889 |
| dense 3D d12 | 12 | r | multiply | 29.196 | 0.221 | 0.286 |
| dense 3D d12 | 12 | r | exact divide | 36.732 | 4.662 | 2.835 |
| dense 3D d12 | 63 | i | multiply | 37.487 | 3.844 | 5.327 |
| dense 3D d12 | 63 | i | exact divide | 41.843 | 7.469 | 9.234 |
| dense 3D d12 | 63 | r | multiply | 86.261 | 9.574 | 6.592 |
| dense 3D d12 | 63 | r | exact divide | 94.784 | 13.104 | 9.794 |
| dense 3D d12 | 127 | i | multiply | 62.508 | 9.825 | 6.651 |
| dense 3D d12 | 127 | i | exact divide | 74.459 | 12.747 | 9.902 |
| dense 3D d12 | 127 | r | multiply | 110.494 | 6.470 | 7.634 |
| dense 3D d12 | 127 | r | exact divide | 128.070 | 11.456 | 11.241 |
| dense 3D d20 | 12 | i | multiply | 183.670 | 2.566 | 3.209 |
| dense 3D d20 | 12 | i | exact divide | 194.345 | 47.840 | 42.362 |
| dense 3D d20 | 12 | r | multiply | 325.213 | 1.729 | 3.180 |
| dense 3D d20 | 12 | r | exact divide | 422.451 | 47.458 | 42.688 |
| dense 3D d20 | 63 | i | multiply | 393.835 | 37.282 | 84.840 |
| dense 3D d20 | 63 | i | exact divide | 470.182 | 67.869 | 141.176 |
| dense 3D d20 | 63 | r | multiply | 926.063 | 95.983 | 108.341 |
| dense 3D d20 | 63 | r | exact divide | 1051.720 | 128.900 | 158.157 |
| dense 3D d20 | 127 | i | multiply | 1292.635 | 185.508 | 108.248 |
| dense 3D d20 | 127 | i | exact divide | 1478.957 | 247.479 | 155.886 |
| dense 3D d20 | 127 | r | multiply | 1411.676 | 72.493 | 123.475 |
| dense 3D d20 | 127 | r | exact divide | 1603.698 | 135.847 | 175.332 |
| sparse 8D d128 | 12 | i | multiply | 0.029 | 0.014 | 0.024 |
| sparse 8D d128 | 12 | i | exact divide | 0.133 | 0.100 | 0.024 |
| sparse 8D d128 | 12 | r | multiply | 0.036 | 0.018 | 0.020 |
| sparse 8D d128 | 12 | r | exact divide | 0.171 | 0.098 | 0.023 |
| sparse 8D d128 | 63 | i | multiply | 0.027 | 0.014 | 0.031 |
| sparse 8D d128 | 63 | i | exact divide | 0.178 | 0.100 | 0.048 |
| sparse 8D d128 | 63 | r | multiply | 0.039 | 0.058 | 0.030 |
| sparse 8D d128 | 63 | r | exact divide | 0.263 | 0.156 | 0.051 |
| sparse 8D d128 | 127 | i | multiply | 0.119 | 0.058 | 0.031 |
| sparse 8D d128 | 127 | i | exact divide | 0.301 | 0.143 | 0.054 |
| sparse 8D d128 | 127 | r | multiply | 0.109 | 0.047 | 0.033 |
| sparse 8D d128 | 127 | r | exact divide | 0.401 | 0.155 | 0.057 |

Sources are `target/symbolica-primitives-harness/poly_shapes.rs` and
`poly_shapes_flint.c`; exact operands are in
`target/parity-audit-20260907/native-polynomial-inputs/`. Raw measurements are
`native-polynomial-symbolica-1.csv` and `native-polynomial-flint-1.csv` beside
that directory. The compiled baseline Symbolica harness SHA-256 is
`288724babc01504684845688cb5e8acfbdb02679e19ec6efb004572f80639f91`.
Optional shape/bit/domain filters were subsequently added to the source without
changing the generator or timed operation.

## Bulk fraction-kernel controls: every case

The following same-binary diagnostic compares stage-2 generic coefficient
accumulation with its optional dense fraction kernel (including fallback).
Each cell averages 20 calls on CPU 56; these are not repeated-pair medians.
Every returned kernel result was checked exactly against the generic result.
The dense support has 455×364 terms and a 24³ dense layout. Sparse inputs have
32×32 products with no collisions. Tiny inputs have 3×2 products. `many` uses
varying odd denominators starting at 101, not necessarily distinct primes.

| Shape | b | Denominators | Generic µs | Kernel/fallback µs | Speedup |
|---|---:|---|---:|---:|---:|
| dense3_d12 | 12 | one | 1568.744 | 219.791 | 7.137× |
| dense3_d12 | 12 | few | 27549.934 | 453.568 | 60.740× |
| dense3_d12 | 12 | many | 164675.378 | 163940.526 | 1.004× |
| dense3_d12 | 63 | one | 4097.918 | 4086.268 | 1.003× |
| dense3_d12 | 63 | few | 86264.717 | 6081.295 | 14.185× |
| dense3_d12 | 63 | many | 183965.717 | 180470.191 | 1.019× |
| dense3_d12 | 127 | one | 10234.105 | 5282.477 | 1.937× |
| dense3_d12 | 127 | few | 103324.030 | 7825.745 | 13.203× |
| dense3_d12 | 127 | many | 195951.195 | 195057.373 | 1.005× |
| sparse_unique | 12 | one | 28.897 | 24.118 | 1.198× |
| sparse_unique | 12 | few | 103.873 | 101.431 | 1.024× |
| sparse_unique | 12 | many | 139.999 | 139.542 | 1.003× |
| sparse_unique | 63 | one | 29.418 | 29.491 | 0.998× |
| sparse_unique | 63 | few | 105.323 | 105.116 | 1.002× |
| sparse_unique | 63 | many | 142.981 | 140.391 | 1.018× |
| sparse_unique | 127 | one | 179.963 | 125.183 | 1.438× |
| sparse_unique | 127 | few | 466.590 | 465.458 | 1.002× |
| sparse_unique | 127 | many | 607.298 | 615.614 | 0.986× |
| tiny | 12 | one | 0.210 | 0.217 | 0.968× |
| tiny | 12 | few | 0.370 | 0.387 | 0.957× |
| tiny | 12 | many | 0.800 | 0.817 | 0.980× |
| tiny | 63 | one | 0.179 | 0.194 | 0.928× |
| tiny | 63 | few | 0.595 | 0.609 | 0.976× |
| tiny | 63 | many | 1.169 | 0.988 | 1.184× |
| tiny | 127 | one | 0.584 | 0.590 | 0.991× |
| tiny | 127 | few | 2.281 | 2.177 | 1.048× |
| tiny | 127 | many | 3.112 | 3.100 | 1.004× |

Both integer and genuine-rational dense cases benefit where the inner kernel
is applicable. Initially unguarded denominator lifting regressed sparse controls
by 3–6×; the variety and collision guards remove those regressions by declining
before LCM work. Sub-microsecond tiny timings are too short for strong ratio
claims; correctness and immediate fallback are the relevant controls here.
Raw data: `target/parity-audit-20260907/fraction-bulk-final.csv`.

## Remaining verification

A separate diagnostic profile of the sparse eight-variable, nominal-127-bit
family recorded 2,627 samples with zero lost samples. Checked division consumed
70.1% inclusive; generic `heap_division` accounted for 61.3% inclusive/17.2%
self. BTreeMap removal/insertion and BinaryHeap popping were substantial
bookkeeping costs. These percentages include a smaller multiplication phase
and are not comparative timings. Source dispatch only permits its eight-variable
packed-division key for dividend degrees at most 127. The original sparse
family's input degrees 128/127 produce dividend degree 255, forcing the generic
heap. The portable upstream harness now adds separate matched-support boundary
controls with input high degrees 64/63, then a whole-left-operand monomial shift
by `x` in the second case, producing maximum dividend degrees 127 and 128 while
preserving every product collision. Those controls are not retroactively part of the 18-family
baseline table. Artifacts: `native-sparse-integer-div-baseline-report.txt` and
its corresponding profile/data files.

The numbers above establish the causes and candidate directions, not universal
parity. The bulk adapter, exact-division dispatch, integer dispatch repair, and
rational-polynomial API migration must be measured after integration with the
port. Sparse large-integer multiplication/division, denominator-variety fallback
costs, repeated symbolic poles, and cold-start period handling remain distinct
families to track. The broader inventory also includes heavy integrals and
SubTropica inputs requiring preprocessing; these native probes do not replace
those workloads.
