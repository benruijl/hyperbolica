# Hyperbolica-maintained Symbolica patches

## Current selection: main, 2026-10-05

The active dependency is Symbolica 3.0.1 from official `main` at
`75f8350094b90254ee71dc2a391fde0d14b0204a`, plus only
[`symbolica-main-20261005.patch`](symbolica-main-20261005.patch).
This small compatibility patch restores the existing native partial-fraction
module export and constructor regression coverage. Heap scratch recycling is
already upstream; the old scalar constructor workaround is unnecessary.

Use [the current snapshot instructions](SYMBOLICA_SNAPSHOT.md) to reconstruct
and validate the selected tree. The September patches and evidence below are
historical and must not be applied to the new base.

## Historical September 2026 record

## Exponent scratch recycling, 2026-09-08

The selected source now adds [`symbolica-heap-scratch-20260908.patch`](symbolica-heap-scratch-20260908.patch)
after the cumulative development patch. Its SHA-256 is `e8ceb492594793354a5c811a24747f2fec9237e7b35c918247eee6550494aa0d`.
Commit `1a01bdbc166c040d176584a7c9ecd1810e73537a` produces tree `d8925723c37ec35a80a40c6b432b565d747ac29f`.
The generic polynomial heap multiplier recycles exponent slots after duplicate
candidates or consumed heap entries release them, avoiding scratch storage
proportional to all visited coefficient pairs. Heap/cache keys remain immutable
while referenced; arithmetic and output order are preserved.

The regression failed before the fix with 65,536 retained exponent slots for
256 rows, then passed with a linear scratch bound. All 639 native tests and
471 port tests pass; port all-target/all-feature Clippy also passes. The native
test command enables `native_code_generation` because upstream evaluator tests
require it; the production feature selection is unchanged. Updated benchmark
and qbox evidence is in [`qbox-allocation-20260908.md`](../docs/qbox-allocation-20260908.md).

## Cumulative base patch, before scratch recycling

The selected checkout is official upstream base
`fb845d34bda8ccf1fedef6544d3aa46dc24944e3` plus the one-commit
[`symbolica-hyperlica-dev-20260908.patch`](symbolica-hyperlica-dev-20260908.patch).
Its SHA-256 is
`7c76d4741d8eb3c2e6b75837129c898edb974857694163a2605f42fb0392a3e5`.
The cumulative reconstruction commit is
`df7eb980ecd3040ac0fd0bacc37b0f34d74b4701`; the active vendor tip is
`15bd1f6d8f604582e24e6186875b2e12cfa23143`. Both produce the complete tree
`7b1c3dde94a48d77bf072f58a54a610e9964240d`. Independent temporary-index
application to pristine `fb845d34` reproduced that tree with strict whitespace
validation and no vendor worktree/index changes.

The new upstream base includes powered-denominator and polynomial edge-case
fixes from `bd29c4704eaa4ea5754330a7c43d59d2bd50e065`. The cumulative patch
retains fraction arithmetic and coefficient-domain kernels, rational exact
division, rational addition/derivative shortcuts, native factored-coefficient
partial fractions, checked multiplicities and regression/benchmark examples.
It uses upstream's constructor implementation, including its canonical-zero
and removable zero-power-factor representation choices, plus the scalar-only
correction from `6e20db0336fbaa2d84df22ce6ca8ff5ac259f072`.

The five resolved standalone MRE examples are omitted from the current patch;
their native unit tests and historical sources remain preserved. The new
`examples/mre_factored_scalar.rs` is retained. Cleanup commit `15bd1f6` changes
only those five obsolete examples and their documentation; production source,
library tests and Cargo manifests match the tested scalar-fixed implementation.

Pristine native validation passed 604/604 tests. The scalar-corrected
implementation passed 637/637 native tests, 25 focused tests and 17 standalone
invocations before the obsolete-example cleanup. The corrected port passed
449 library tests, its remaining all-target checks, 79 debug and 79 release C++
differential cases (24 normalized), and the all-targets/all-features check.
Corrected release compilation completed; executable SHA-256 is
`cf72945dcada1632e31dca72e883ad91997d741e755b8050af335a16963ffea4`.

The final corrected matrix passed all 162 exact comparisons and repeatability
checks, with all paired runtime ratios favoring Rust. This is measured-corpus
evidence, not a universal or memory-parity claim; see
[`performance-dev-20260908.md`](../docs/performance-dev-20260908.md) for timings
and limits. The preliminary matrix measured `7094dda` before the scalar fix
and remains separate. Historical failures or timings must not be presented as
observations of the corrected final build.
See [`SYMBOLICA.md`](SYMBOLICA.md) for the validation boundary and
[`SYMBOLICA_SNAPSHOT.md`](SYMBOLICA_SNAPSHOT.md) for the current recipe.

## Standalone scalar correction: already included

[`symbolica-hyperlica-scalar-fix-20260908.patch`](symbolica-hyperlica-scalar-fix-20260908.patch)
is retained as the isolated correction against pre-scalar-fix local commit
`7094ddabc18ee535cb8dd169a4976a519f26f8c7`. Its SHA-256 is
`535d483ac29c1688c8b75d07be719b71ded385b25e086630e668a950df5e06bf`.
It avoids sending scalar bases with empty variable maps into polynomial
factorization, adds eighteen scalar-only combinations to the native regression
matrix, and includes the exact `1/2` standalone reproducer.

Do not stack this artifact on the cumulative current patch: its correction is
already included. Use the cumulative patch on pristine `fb845d34` for the
selected source recipe. The final source archive `symbolica-final-source.tar`
has SHA-256
`2da5a6bc1d2ea3a6a61479f009df050105c9a2d9961f6c78ac6ed7ce4d009573`.

## Preserved earlier three-commit series

[`symbolica-hyperlica.patch`](symbolica-hyperlica.patch) is the unchanged
historical series on base `0b57776bf911faeea7e28ea133706fb03740ffeb`, with
SHA-256 `7cb664d6287ce31d13681b75c0bd4f3ec512d95d90ba8624e1c5af397ad35133`.
Its commits are `c4433a33bbcae1639f4c391700f37278875c8da9`,
`d016b123a4c7d463d10e3b56279e2fb66a41f75e`, and
`e33a1fb70abe70e774dd6a55baa5a8e1f42cc8e3`; its resulting tree is
`49dfd8a2b3e16c21ef78425c945578ce8a7815fd`.
It preserves the old implementations, original diagnostic expectations and
provenance. Do not apply it to the new base or stack it with the current patch.

## Historical `dev` integration status

The earlier audit baseline was official `dev` at
`0b57776bf911faeea7e28ea133706fb03740ffeb`. Two improvements formerly
carried here had already been integrated in that history as:

- `4fd8443c` — `Add resultant trait for best algorithm selection per ring`;
- `b8fa6b53` — `Use binary exponentation for rational polynomials`.

The powering commit is byte-identical to the standalone patch below. The
resultant commit contains the same coefficient-domain trait, primitive-integer
Ducos path, CRT path, and rational selector, with the selector's private
organization and documentation streamlined upstream. Applying either patch to
that baseline failed because its changes were already present. Upstream history
has since been rewritten; these old commit IDs are historical provenance, not
ancestry requirements for the newly selected tree.

## Standalone binary-powering patch

[`symbolica-dev-rational-power.patch`](symbolica-dev-rational-power.patch) is
the clean, powering-only patch against predecessor
`bd0c137e62cbd07ebc045bc4b04ac661a14f3be6`. Its SHA-256 is
`452fc0c595207ae37270927b1efcda3ad1c72df3bb58fbd9d03a0fcf5e85d82b`.
It touches only `src/domains/rational_polynomial.rs` and
`tests/rational_polynomial.rs`; resultant changes are excluded.

It replaces linear repeated multiplication with exact exponentiation by
squaring, delegates `RationalPolynomialField::pow` to the same path, and tests
powers 0, 1, and 13 through both public APIs.

## Standalone resultant patch

[`symbolica-dev-resultant.patch`](symbolica-dev-resultant.patch) is the clean,
resultant-only patch against the same predecessor. Its SHA-256 is
`a47d5fd653daaeec17bd4cd2ffb6ccd0f32cf48302a8f6eb9f8fbd2e14688387`.
It touches only `src/lib.rs`, `src/poly.rs`, and `src/poly/resultant.rs`; binary
powering is excluded. Its native tests cover rational contents, denominators,
signs, zero/constant/small inputs, the internal Ducos/CRT boundary, and public
trait dispatch.

## Combined predecessor patch

[`symbolica-dev.patch`](symbolica-dev.patch) combines the two disjoint patches
above. Its SHA-256 is
`5f5a3ad9a0b4552772097581eea9673e7a2d1e896f3996d5afb2d439972d90d0`.
All three artifacts were checked with strict whitespace validation against a
temporary index at `bd0c137`; their post-images match the former nested
checkout exactly, and the two standalone patches reproduce the combined
post-image.

To inspect or apply one of these historical patches, first check out its exact
base, not current `dev`:

```sh
git -C vendor/symbolica checkout bd0c137e62cbd07ebc045bc4b04ac661a14f3be6
git -C vendor/symbolica apply --check ../symbolica-dev-resultant.patch
git -C vendor/symbolica apply ../symbolica-dev-resultant.patch
```

Substitute `symbolica-dev-rational-power.patch` or `symbolica-dev.patch` as
needed. The corresponding focused regression commands used for the historical
baseline were:

```sh
cargo test --manifest-path vendor/symbolica/Cargo.toml \
  --test rational_polynomial \
  rational_polynomial_power_uses_exact_binary_exponentiation
cargo test --manifest-path vendor/symbolica/Cargo.toml --lib \
  resultant_integer_ducos -- --test-threads=1
cargo test --manifest-path vendor/symbolica/Cargo.toml --lib \
  rational_resultant -- --test-threads=1
```

The exact scalar-content `from_num_den(..., true)` regression and the F4
same-matrix regression also passed on the pristine historical baseline. Commit `bd0c137`
contains the formerly local factorized-`is_one` and rational-derivative
scalar-normalization fixes, so Hyperbolica carries no workaround for either.

## Archived `symbolica-src` patch record

The sections below record the patches formerly carried on the archived source
copy at `61f88b381247f5f25dd7832b0286b5e362bd4148`. Paths and commands in this
archived section intentionally refer to `vendor/symbolica-src`; they are kept
for provenance and are not instructions for the selected build.

## Rational-polynomial scalar-content normalization

### Why the patch is needed

`RationalPolynomial::from_num_den` reduces integer numerator and denominator
polynomials by their polynomial GCD. A valid polynomial-GCD implementation may
return a primitive associate rather than including the shared scalar content.
The remaining numerator and denominator can therefore retain the same integer
factor—for example, `21*(1+3*x^2+x^20) / (21*(1+y))`.

Those values are mathematically equal to their primitive representation, but
the unreduced scalar makes structural equality and hashing disagree. In
Hyperbolica this surfaced when a native polynomial-part round trip produced an
equivalent rational function with a different internal scale.

The patch removes the GCD of the two polynomial contents after polynomial-GCD
reduction and before denominator-sign normalization. It affects only the
integer-backed rational-polynomial constructor.

### Patch

Apply from the repository root with `git apply` after saving the following
diff, or reproduce the two edits directly.

```diff
diff --git a/vendor/symbolica-src/src/domains/rational_polynomial.rs b/vendor/symbolica-src/src/domains/rational_polynomial.rs
--- a/vendor/symbolica-src/src/domains/rational_polynomial.rs
+++ b/vendor/symbolica-src/src/domains/rational_polynomial.rs
@@
             }
 
+            // Polynomial GCD implementations are allowed to return a
+            // primitive associate.  In that case a scalar common content can
+            // remain in both integer polynomials, making equal rational
+            // functions compare and hash differently.  Remove that content
+            // explicitly so the integer-backed representation is canonical.
+            let content = num.ring().gcd(&num.content(), &den.content());
+            if !num.ring().is_one(&content) {
+                num = num.div_coeff(&content);
+                den = den.div_coeff(&content);
+            }
+
             // normalize denominator to have positive leading coefficient
             if den.lcoeff().is_negative() {
                 num = -num;
diff --git a/vendor/symbolica-src/tests/rational_polynomial.rs b/vendor/symbolica-src/tests/rational_polynomial.rs
--- a/vendor/symbolica-src/tests/rational_polynomial.rs
+++ b/vendor/symbolica-src/tests/rational_polynomial.rs
@@
-        rational_polynomial::{RationalPolynomial, RationalPolynomialField},
+        rational_polynomial::{
+            FromNumeratorAndDenominator, RationalPolynomial, RationalPolynomialField,
+        },
@@
 fn rational_polynomial_power_uses_exact_binary_exponentiation() {
     // Existing test body omitted from this patch note.
 }
 
+#[test]
+fn integer_rational_polynomial_removes_common_scalar_content() {
+    let variables = Arc::new(vec![
+        PolyVariable::Symbol(symbol!("content_x")),
+        PolyVariable::Symbol(symbol!("content_y")),
+    ]);
+    let numerator = parse!("21+63*content_x^2+21*content_x^20")
+        .to_polynomial::<_, u16>(&Z, Some(variables.clone()));
+    let denominator =
+        parse!("21+21*content_y").to_polynomial::<_, u16>(&Z, Some(variables.clone()));
+
+    let normalized = RationalPolynomial::from_num_den(numerator, denominator, &Z, true);
+    let expected: RationalPolynomial<_> = parse!("(1+3*content_x^2+content_x^20)/(1+content_y)")
+        .to_rational_polynomial(&Z, &Z, Some(variables));
+
+    assert_eq!(normalized, expected);
+}
```

### Verification

With `SYMBOLICA_LICENSE` set in the environment, run:

```sh
cargo test --manifest-path vendor/symbolica-src/Cargo.toml \
  --test rational_polynomial \
  integer_rational_polynomial_removes_common_scalar_content

cargo test --lib \
  core::rat::tests::native_polynomial_part_integral_round_trips_without_q_views
```

The first command is the focused Symbolica regression. The second verifies the
Hyperbolica operation that originally exposed the non-canonical representation.

## Native F4 fix and defensive completion fallback

### Why the patch is needed

Symbolica's F4 implementation can finish with an incomplete basis because it
updates its live basis while it is still registering simplification rules from
one Macaulay matrix. A row created by that matrix can consequently be treated
as a reducer for a later row from the same matrix. Sharing a leading monomial
is not enough to justify that rewrite: the difference can be the critical
S-polynomial that F4 still needs to process.

The minimized case in `SYMBOLICA_F4_MRE.md` makes this concrete. The first
matrix creates `h=x*y+y^2`, then incorrectly registers
`y*(x*z+27358*y)` as the simplification of `z*h`. Their difference is the
missing polynomial `y^2*z+38163*y^2` over `GF(65521)`. Exact
`GroebnerBasis::is_groebner_basis` verification therefore fails without any
external oracle.

The exact introducing commit is
`a49b86ac364c684281a594bc88cfc7022766746c` (2023-11-08). Its parent
`c1bbc0e77b9d3d28cacffcabf2bc236db626b23d` passes the two-binomial MRE; the
child fails it. Public Symbolica `dev` at
`f9f756250201a13d2b06a5a75b383cb1e522c69f` still contains the defect.

The native fix snapshots the basis after echelonization and uses that
pre-matrix snapshot while registering simplification rules. Newly generated
rows are still added to the live basis and participate in subsequent critical
pairs, but cannot rewrite another row from their own matrix.

### Native F4 patch

```diff
diff --git a/vendor/symbolica-src/src/poly/groebner.rs b/vendor/symbolica-src/src/poly/groebner.rs
--- a/vendor/symbolica-src/src/poly/groebner.rs
+++ b/vendor/symbolica-src/src/poly/groebner.rs
@@
             echelonize(
                 // arguments omitted
             );
 
+            // Simplification rules derived from this matrix may only rewrite
+            // multiples of the basis that existed when the matrix was built.
+            // A new row is not yet a valid reducer for another row from the
+            // same matrix: registering it here can replace both sides of an
+            // unprocessed S-pair by the same cached polynomial.
+            let simplification_basis = basis.clone();
+
             // construct new polynomials
             for m in &matrix {
@@
-                    'bf: for (g_ind, g) in &basis {
+                    'bf: for (g_ind, g) in &simplification_basis {
```

### Defensive exact-verification fallback

The companion patch exposed `ensure_groebner_basis`, which verified an F4
result using Symbolica's exact S-polynomial check. If verification failed, a
conventional Buchberger work list added only the missing remainders, then the
existing basis reducer canonicalized the completed result. Verification was
explicit because checking all S-polynomials of a large valid basis is not free.

At that archived revision, Hyperbolica invoked the method on every reduced
Euler basis because an incomplete leading ideal could also retain pure powers
and produce a wrong finite count. Verification operated on the already reduced
result and used Buchberger's product criterion to skip relatively-prime leading
monomials. The later historical `0b57776` baseline contained the native fix and
needed no fallback.

### Defensive fallback patch

```diff
diff --git a/vendor/symbolica-src/src/poly/groebner.rs b/vendor/symbolica-src/src/poly/groebner.rs
--- a/vendor/symbolica-src/src/poly/groebner.rs
+++ b/vendor/symbolica-src/src/poly/groebner.rs
@@
         b.f4();
         b.reduce_basis()
     }
+
+    /// Verify this F4 result and complete it if a critical S-polynomial was
+    /// missed.
+    ///
+    /// Exact verification can be expensive for a large valid basis, so it is
+    /// explicit rather than part of [`Self::new`]. Callers that use a missing
+    /// leading monomial to make a semantic decision should invoke this method
+    /// before accepting that decision.
+    pub fn ensure_groebner_basis(mut self) -> Self {
+        if !Self::is_groebner_basis(&self.system) {
+            self.complete_with_buchberger();
+            self = self.reduce_basis();
+        }
+        self
     }
@@
     pub fn is_groebner_basis(system: &[MultivariatePolynomial<R, E, O>]) -> bool {
         for (i, p1) in system.iter().enumerate() {
             for p2 in &system[i + 1..] {
+                if p1
+                    .max_exp()
+                    .iter()
+                    .zip(p2.max_exp())
+                    .all(|(first, second)| *first == E::zero() || *second == E::zero())
+                {
+                    continue;
+                }
                 let lcm: Vec<E> = p1
     }
     }
+
+    /// Complete an incomplete F4 result using Buchberger's criterion.
+    ///
+    /// This is a correctness fallback, not the primary basis algorithm.  It
+    /// is called only when the much faster F4 result fails the exact
+    /// S-polynomial verification above.
+    fn complete_with_buchberger(&mut self) {
+        let mut pairs = (0..self.system.len())
+            .flat_map(|right| (0..right).map(move |left| (left, right)))
+            .collect::<Vec<_>>();
+
+        while let Some((left, right)) = pairs.pop() {
+            let first = self.system[left].clone();
+            let second = self.system[right].clone();
+            let lcm = first
+                .max_exp()
+                .iter()
+                .zip(second.max_exp())
+                .map(|(first, second)| *first.max(second))
+                .collect::<Vec<_>>();
+            let first_multiplier = lcm
+                .iter()
+                .zip(first.max_exp())
+                .map(|(lcm, exponent)| *lcm - *exponent)
+                .collect::<Vec<_>>();
+            let second_multiplier = lcm
+                .iter()
+                .zip(second.max_exp())
+                .map(|(lcm, exponent)| *lcm - *exponent)
+                .collect::<Vec<_>>();
+
+            if first_multiplier == second.max_exp()
+                && second_multiplier == first.max_exp()
+            {
+                continue;
+            }
+
+            let first_scale = first.ring().inv(first.max_coeff());
+            let second_scale = second.ring().inv(second.max_coeff());
+            let s_polynomial = first
+                .mul_exp(&first_multiplier)
+                .mul_coeff(first_scale)
+                - second
+                    .mul_exp(&second_multiplier)
+                    .mul_coeff(second_scale);
+            let remainder = s_polynomial.reduce(&self.system);
+            if remainder.is_zero() {
+                continue;
+            }
+
+            let scale = remainder.ring().inv(remainder.max_coeff());
+            let remainder = remainder.mul_coeff(scale);
+            let new_index = self.system.len();
+            pairs.extend((0..new_index).map(|index| (index, new_index)));
+            self.system.push(remainder);
+        }
+
+        debug_assert!(Self::is_groebner_basis(&self.system));
+    }
```

The focused native regression constructs the minimized two-binomial system
over `GF(65521)` and asserts that raw `GroebnerBasis::new` returns the missing
`y^2*z` leading monomial, passes exact Gröbner verification, and preserves the
input ideal. The original four-polynomial Euler-derived system is retained as
a broader regression. A deterministic property test checks 96 generated small
systems over six prime fields with the same two exact invariants.

### Verification

With `SYMBOLICA_LICENSE` set in the environment, run:

```sh
cargo test --manifest-path vendor/symbolica-src/Cargo.toml --lib \
  f4_does_not_cache_a_same_matrix_row_as_a_simplification_rule

cargo test --manifest-path vendor/symbolica-src/Cargo.toml --lib \
  f4_random_small_systems_pass_exact_buchberger_verification

cargo test --manifest-path vendor/symbolica-src/Cargo.toml --lib \
  poly::groebner::test

cargo test --lib algebra::euler:: -- --test-threads=1
```

The first command is the minimized native F4 regression. The second performs
the deterministic randomized exact check. The third covers every Symbolica
Gröbner/solve unit test. The final command covers the Hyperbolica Euler systems
that originally returned incorrect positive-dimensional statuses.

A release-mode fixed-versus-unfixed comparison on public `dev` used the same
cyclic-4 GrevLex workload over `GF(65521)`, five alternating CPU-pinned samples,
and 100,000 basis constructions per sample. The unfixed mean was 45.236 us per
basis and the fixed mean was 45.795 us; medians were 45.256 us and 45.770 us.
The native fix was therefore 1.24% slower by mean and 1.13% by median, with the
same 30 output terms. This is within the run-to-run performance envelope and
does not include the separate explicit verification fallback.

## Refreshing the vendor snapshot

When updating Symbolica, advance the clean `dev` checkout, pin its exact
revision in `SYMBOLICA_SNAPSHOT.md` and `check-pure-symbolica.sh`, then run the
focused upstream regressions and require a clean nested worktree. Rerun
Hyperbolica's full correctness and performance gates before accepting the new
revision. Create a new base-specific patch and record its hash only if a needed
change is still absent upstream; do not apply the historical predecessor
patches to current `dev`.
