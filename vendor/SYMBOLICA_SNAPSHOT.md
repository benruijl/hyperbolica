# Symbolica `dev_poly` checkout

Hyperbolica builds against an official Symbolica Git checkout at
`vendor/symbolica`. The directory is intentionally ignored by the parent
repository; Cargo selects it directly rather than the archived
`vendor/symbolica-src` copy.

- Remote: `https://github.com/symbolica-dev/symbolica.git`
- Branch: `dev_poly`
- Audited revision: `76e3eb630abcc4d597463d759a0b40fedb57b764`
- Symbolica package version: `2.2.0`
- Audit date: 2026-09-01

Create the checkout and apply the recorded Hyperbolica patch with:

```sh
git clone --branch dev_poly --single-branch \
  https://github.com/symbolica-dev/symbolica.git vendor/symbolica
git -C vendor/symbolica checkout 76e3eb630abcc4d597463d759a0b40fedb57b764
git -C vendor/symbolica switch -C dev_poly
git -C vendor/symbolica apply ../symbolica-dev_poly.patch
scripts/check-pure-symbolica.sh
```

The revision is pinned because `dev_poly` is a moving branch. Updating it
requires refreshing `vendor/symbolica-dev_poly.patch`, this record, and all
correctness and performance evidence. The source/patch identity check in
`scripts/check-pure-symbolica.sh` prevents unnoticed checkout drift.

## Optimized-source provenance

On 2026-09-01, `git ls-remote --heads origin dev_poly`, the local branch, and
the fetched remote-tracking branch all resolved to the audited revision above
(ahead/behind `0/0`). Cargo metadata resolved `symbolica`, `numerica`, and
`graphica` to this checkout, and the release dependency file contained
`vendor/symbolica/` paths with no `vendor/symbolica-src/` path.

The checkout contains the recent polynomial/resultant optimization ancestry,
including:

- `fe3805d` — Lazard--Ducos and small-resultant improvements;
- `c20c92f` — fused Ducos steps and modular CRT resultants;
- `bb3f1fc` — polynomial multiplication, storage, division, and resultant
  improvements; and
- `0c54f8b` — the final public Ducos/CRT backend organization used by the
  comparison harness.

The local patch adds a thin rational-to-primitive-integer adapter and an
internal rational-resultant selector in `src/poly/resultant.rs`; both call the
selected checkout's current Ducos and CRT kernels unchanged. Numerica's
polynomial kernels, the benchmark sources, and the checked-in PolyBench results
are untouched. The root crate enables
Symbolica's `faster_alloc`, GMP-integer, and MPFR-float features; none of the
omitted features selects an alternative polynomial kernel.

One upstream evidence caveat is retained explicitly: the final PolyBench
README names short source hash `0352f1b`, but that object is not resolvable in
this clone or any advertised remote reference. The optimized source lineage
itself is present and auditable; that one artifact-to-source hash is not.

## Upstream fixes already present

Two formerly local correctness fixes are part of this `dev_poly` revision and
are not repeated in the patch:

- Integer `RationalPolynomial::from_num_den(..., true)` removes the common
  scalar content from numerator and denominator even when their active
  variables are disjoint. Symbolica's native
  `integer_content_is_removed_from_disjoint_variables` regression passes for
  the exact `21*(1+3*x^2+x^20) / (21*(1+y))` case.
- F4 uses a fixed pre-matrix basis when recording simplification rules. The fix
  is Symbolica commit `ec19eeb211685aa216dbd28a4df547cd4c6baca1`; both the
  native `prevent_live_basis_update` regression and Hyperbolica's minimized F4
  example pass. Hyperbolica therefore calls `GroebnerBasis::new` directly and
  no longer carries `ensure_groebner_basis` or its Buchberger fallback.

Symbolica's derivative still creates quotient-rule intermediates with
`do_gcd = false`. Differentiating
`(x^21/21+x^3+x)/(y+1)` can consequently return a structurally noncanonical
common scalar even though the `do_gcd = true` constructor is fixed.
Hyperbolica removes only that scalar at its derivative boundary; the focused
constructor and integration round-trip regressions both pass.

## Hyperbolica patch still applied

[`symbolica-dev_poly.patch`](symbolica-dev_poly.patch) contains three maintained
changes not present upstream:

- rational-polynomial powers use exponentiation by squaring and the field
  implementation delegates to the same kernel;
- `FactorizedRationalPolynomialField::is_one` also checks `numer_coeff`.
  Without that check a scalar such as `2` is misclassified as one and native
  factorized partial fractions can enter monic polynomial division with a
  nonmonic divisor;
- rational-coefficient multivariate resultants can clear denominators and
  content once, compute over primitive integer associates, and select the
  integer-Ducos or CRT kernel inside Symbolica.

All three changes have focused Symbolica tests in the patch. The patch file is the
complete expected dirty-tree diff of the nested checkout.

## Archived source copy

`vendor/symbolica-src` is the previous copied snapshot at
`61f88b381247f5f25dd7832b0286b5e362bd4148`. It remains only as an audit and
historical patch reference and is not selected by Cargo. Do not use its test
results or performance numbers as evidence for the current build.
