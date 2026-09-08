# Pinned Symbolica checkout

Hyperbolica builds against a patched official Symbolica Git checkout at
`vendor/symbolica`. The directory is intentionally ignored by the parent
repository; Cargo selects it directly rather than the archived
`vendor/symbolica-src` copy.

- Remote: `https://github.com/symbolica-dev/symbolica.git`
- Upstream base: `0b57776bf911faeea7e28ea133706fb03740ffeb` (`dev`)
- Local audited tip: `e33a1fb70abe70e774dd6a55baa5a8e1f42cc8e3`
- Audited source tree: `49dfd8a2b3e16c21ef78425c945578ce8a7815fd`
- Symbolica package version: `2.2.0`
- Worktree diff: none; three local commits are preserved in `symbolica-hyperlica.patch`
- Packaging date: 2026-09-08

Create the exact clean checkout with:

```sh
git clone --branch dev --single-branch \
  https://github.com/symbolica-dev/symbolica.git vendor/symbolica
git -C vendor/symbolica checkout 0b57776bf911faeea7e28ea133706fb03740ffeb
git -C vendor/symbolica switch -c codex/hyperlica-rational-fastpaths
git -C vendor/symbolica am ../symbolica-hyperlica.patch
scripts/check-pure-symbolica.sh
```

The source tree is pinned because `dev` is a moving branch. Updating it requires
refreshing this record and all correctness and performance evidence. The clean
source identity check in `scripts/check-pure-symbolica.sh` prevents unnoticed
checkout drift or local modifications. Applying the mail patch can produce
different commit IDs due to committer metadata, but must produce the pinned
source tree. See [`SYMBOLICA.md`](SYMBOLICA.md) for the patch hash and bug MREs.

## Original upstream baseline provenance

On 2026-09-02, `git ls-remote --heads origin dev`, the local branch, and
the fetched remote-tracking branch all resolved to the audited revision above
(ahead/behind `0/0`). Cargo metadata resolved `symbolica`, `numerica`, and
`graphica` to this checkout, and the release dependency file contained
`vendor/symbolica/` paths with no `vendor/symbolica-src/` path.

The checkout contains the consolidated optimized polynomial/resultant source
in `df088bc` and the matching low-level Numerica kernels in `e029754`. Newer
`dev` commits through `89749e2` further revise polynomial GCD, factorization,
interpolation, packed products, and Hensel lifting. The maintained performance
gate measures this exact revision rather than inferring speed from commit
ancestry alone.

The `dev` and former `dev_poly` histories were imported/squashed rather than
joined by literal Git ancestry. Comparing former `dev_poly` revision `76e3eb6`
with predecessor `dev` revision `bd0c137`, Git object IDs prove byte-identical
blobs for `src/poly/{resultant,gcd,polynomial,factor}.rs` and a byte-identical
complete `lib/numerica` tree. Current `dev` at `0b57776` contains later source
changes as well as the complete resultant and power improvements:

- `4fd8443c` adds the public coefficient-domain `PolynomialResultant` trait,
  the primitive-integer rational adapter, and internal integer-Ducos/CRT
  selection. `RationalField` owns the specialized selector, `IntegerRing`
  explicitly opts into the generic default, and there is no overlapping
  blanket field implementation on stable Rust.
- `b8fa6b53` replaces linear rational-polynomial powering with exponentiation
  by squaring and delegates the coefficient-field implementation to it.

Both commits are ancestors of the upstream baseline. The new HyperLica series
adds further rational and polynomial changes and standalone diagnostics; it
does not change the existing checked-in PolyBench results. The root crate enables
Symbolica's `faster_alloc`, GMP-integer, and MPFR-float features; none of the
omitted features selects an alternative polynomial kernel.

## Upstream fixes already present

Four formerly local correctness workarounds are part of this `dev` revision
and require no local workaround. They are also outside the two historical
resultant/power artifacts:

- Integer `RationalPolynomial::from_num_den(..., true)` removes the common
  scalar content from numerator and denominator even when their active
  variables are disjoint. Symbolica's native
  `integer_content_is_removed_from_disjoint_variables` regression passes for
  the exact `21*(1+3*x^2+x^20) / (21*(1+y))` case.
- F4 uses a fixed pre-matrix basis when recording simplification rules. The fix
  is Symbolica commit `1f6621d`; both the
  native `prevent_live_basis_update` regression and Hyperbolica's minimized F4
  example pass. Hyperbolica therefore calls `GroebnerBasis::new` directly and
  no longer carries `ensure_groebner_basis` or its Buchberger fallback.
- `FactorizedRationalPolynomialField::is_one` includes `numer_coeff`, so a
  scalar such as `2` is no longer misclassified as one.
- Rational-polynomial differentiation constructs both quotient-rule terms
  with `do_gcd = true`, removing scalar content exposed by differentiation.
  Both fixes are in Symbolica commit `bd0c137`, so Hyperbolica no longer carries
  either workaround.

## Former Hyperbolica patches now upstream

The three patch files retained beside this document are provenance artifacts
against predecessor `bd0c137e62cbd07ebc045bc4b04ac661a14f3be6`:

- `symbolica-dev-resultant.patch` contains only the resultant work;
- `symbolica-dev-rational-power.patch` contains only binary powering;
- `symbolica-dev.patch` is their combined historical patch.

They are deliberately not applied to current `dev`, which already contains
both changes. See [`SYMBOLICA_PATCH.md`](SYMBOLICA_PATCH.md) for exact hashes,
scope, upstream commit IDs, and predecessor application instructions.

## Archived source copy

`vendor/symbolica-src` is the previous copied snapshot at
`61f88b381247f5f25dd7832b0286b5e362bd4148`. It remains only as an audit and
historical patch reference and is not selected by Cargo. Do not use its test
results or performance numbers as evidence for the current build.
