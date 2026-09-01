# Hyperbolica-maintained Symbolica patches

## Current `dev_poly` patch

The selected dependency is the official `dev_poly` checkout at
`vendor/symbolica`, audited at
`76e3eb630abcc4d597463d759a0b40fedb57b764`. Its complete local diff is the
machine-applicable [`symbolica-dev_poly.patch`](symbolica-dev_poly.patch).
Apply it from the repository root after creating the checkout:

```sh
git -C vendor/symbolica apply ../symbolica-dev_poly.patch
```

The current patch contains three changes:

1. `RationalPolynomial::pow` uses exponentiation by squaring, and the
   `RationalPolynomialField` implementation delegates to it. Pristine
   `dev_poly` still performs `e` multiplications. The focused
   `rational_polynomial_power_uses_exact_binary_exponentiation` test covers
   exponents 0, 1, and 13 through both APIs.
2. `FactorizedRationalPolynomialField::is_one` checks `numer_coeff`. Pristine
   `dev_poly` misclassifies the scalar `2` because its polynomial payload is
   one and the scalar lives in `numer_coeff`. This can route native factorized
   partial fractions into monic division with a nonmonic divisor. The focused
   `field_is_one_checks_the_numerator_coefficient` test and Hyperbolica's
   factored-partial-fraction suite cover the fix.
3. `UnivariatePolynomial<PolynomialRing<RationalField, _>>` exposes
   `resultant_ducos_integer` and `resultant_auto`. The former clears scalar
   denominators and global integer contents once, runs the checkout's optimized
   Ducos recurrence over `Z[parameters]`, and restores the exact homogeneous
   rational scale. The latter owns the coefficient-domain-specific choice
   between that path and CRT, analogous to Symbolica's internal polynomial-GCD
   planning; Hyperbolica does not duplicate the guard. Native tests cover the
   dispatch boundary, denominators, nontrivial contents and signs, swapped odd
   degrees, zero, constants, and the small-resultant formulas. The existing CRT
   adapter shares the same primitive-integer conversion.

The exact scalar-content `from_num_den(..., true)` regression and the F4
same-matrix regression already pass on pristine `dev_poly`, so their old local
patches are not applied to the selected checkout. Symbolica's derivative still
uses `do_gcd = false` internally; Hyperbolica performs a targeted scalar
normalization at that wrapper boundary.

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

The companion patch exposes `ensure_groebner_basis`, which verifies an F4
result using Symbolica's exact S-polynomial check. If verification fails, a conventional
Buchberger work list adds only the missing remainders, then the existing basis
reducer canonicalizes the completed result. Verification is explicit because
checking all S-polynomials of a large valid basis is not free.

Hyperbolica invokes the method on every reduced Euler basis because an
incomplete leading ideal can also retain pure powers and produce a wrong finite
count. Verification operates on the already reduced result and uses
Buchberger's product criterion to skip relatively-prime leading monomials.

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

When updating Symbolica, first run the focused tests without these patches.
Drop any patch whose regression now passes against upstream unchanged;
otherwise reapply it and update the snapshot commit recorded in
`SYMBOLICA_SNAPSHOT.md`. Always rerun Hyperbolica's full correctness and
performance gates because exact F4 verification adds work on every path where
the caller requests it, even with the product-criterion fast path.
