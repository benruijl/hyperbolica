# Pinned Symbolica checkout

Hyperbolica builds against a patched official Symbolica Git checkout at
`vendor/symbolica`. The directory is intentionally ignored by the parent
repository; Cargo selects it directly rather than the archived
`vendor/symbolica-src` copy.

- Remote: `https://github.com/symbolica-dev/symbolica.git`
- Upstream base: `fb845d34bda8ccf1fedef6544d3aa46dc24944e3` (fetched `dev`, 2026-09-08)
- Upstream source tree: `d232cb41a72ec749f1e1ce23b0088a307deca45f`
- Active vendor tip: `1a01bdbc166c040d176584a7c9ecd1810e73537a`
- Scalar-corrected implementation: `6e20db0336fbaa2d84df22ce6ca8ff5ac259f072`
- Cumulative reconstruction commit: `df7eb980ecd3040ac0fd0bacc37b0f34d74b4701`
- Selected source tree: `d8925723c37ec35a80a40c6b432b565d747ac29f`
- Symbolica package version: `2.2.0`
- Worktree diff: none; apply the cumulative development patch, then the scratch-recycling patch
- Cumulative patch SHA-256: `7c76d4741d8eb3c2e6b75837129c898edb974857694163a2605f42fb0392a3e5`
- Scratch follow-up patch SHA-256: `e8ceb492594793354a5c811a24747f2fec9237e7b35c918247eee6550494aa0d`
- Packaging date: 2026-09-08
- Native validation: pristine 604/604; scalar-corrected 637/637, 25 focused tests and 17 standalone invocations passed
- Scalar-corrected port validation: 449 library tests, remaining all-target checks, 79 debug and 79 release differential cases passed (24 normalized)
- `--all-targets --all-features` check: passed
- Corrected release executable SHA-256: `cf72945dcada1632e31dca72e883ad91997d741e755b8050af335a16963ffea4`
- Final performance matrix: 162 exact comparisons passed; all 162 paired runtime ratios favored Rust in this run

See [`performance-dev-20260908.md`](../docs/performance-dev-20260908.md) for
timings, memory caveats and separate heavy-case coverage. The preliminary
162-case run predates the scalar fix and remains a distinct historical result.

Create the exact clean checkout with:

```sh
git clone --branch dev --single-branch \
  https://github.com/symbolica-dev/symbolica.git vendor/symbolica
git -C vendor/symbolica checkout fb845d34bda8ccf1fedef6544d3aa46dc24944e3
git -C vendor/symbolica switch -c codex/hyperlica-dev-20260908
git -C vendor/symbolica am ../symbolica-hyperlica-dev-20260908.patch
git -C vendor/symbolica am ../symbolica-heap-scratch-20260908.patch
scripts/check-pure-symbolica.sh
```

The source tree is pinned because `dev` is a moving branch. Updating it requires
refreshing this record and all correctness and performance evidence. The clean
source identity check in `scripts/check-pure-symbolica.sh` prevents unnoticed
checkout drift or local modifications. Applying the mail patch can produce
different commit IDs due to committer metadata, but must produce the pinned
source tree. The cumulative patch reconstructs the earlier base tree; the follow-up patch
adds exponent scratch recycling to reach the selected tree.
Independent temporary-index application of both patches to `fb845d34`
reproduced `d8925723` exactly with strict whitespace checks. See
[`SYMBOLICA.md`](SYMBOLICA.md) for validation boundaries and the scalar MRE.

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

## Refreshed upstream and retained local changes

Upstream `bd29c4704eaa4ea5754330a7c43d59d2bd50e065`, included in the selected
base, supplies the audited powered-denominator, scalar-unit, duplicate-factor,
zero-denominator, constant-context and sparse-partial-fraction corrections.
Its zero-operand univariate GCD normalization addresses the old randomized
Galois-upgrade failure. The selected local patch retains the generic arithmetic
and polynomial fast paths, rational exact division, factored-coefficient
partial fractions, checked multiplicities and extra regression tests.
It does not reapply the old local constructor implementation.

The local scalar correction bypasses polynomial factorization for scalar
denominator bases, leaving domain-specific scalar normalization in charge.
It fixes zero-variable construction of exact `1/2`; the standalone regression
is `examples/mre_factored_scalar.rs`. The cumulative patch already includes
this fix. Do not also apply `symbolica-hyperlica-scalar-fix-20260908.patch`.

The root-isolation unit test follows the corrected upstream initial
`[3/16, 9/32]` interval, not the historical `[15/64, 9/32]` expectation. Its
refined interval is unchanged. The five resolved standalone MRE examples were
removed in cleanup commit `15bd1f6`, while native tests and all production code
were preserved. Their historical sources and run logs remain separate evidence.
Old failure counts are not current failures of the freshly validated source.

Upstream history has been rewritten since the earlier baseline. Old commit
ancestry is not a valid source-identity gate for this refresh. Pin the new base
and complete patched tree, retain the old artifacts separately, and rerun the
functional regressions. Do not apply either `symbolica-hyperlica.patch` or the
older resultant/power patches on top of the selected current patch.

The current `License.md` differs from the previous notice. Its SHA-256 is
`95576170a17bc8996fb8a94473745545048b6edc5d91e3867f091f7fc6b26981`;
see that complete bundled file and the parent distribution notice. Numerica
and Graphica retain their separate `License.md` files. This is a provenance
record, not an interpretation of license terms.

## Original upstream baseline provenance (historical)

On 2026-09-02, `git ls-remote --heads origin dev`, the local branch, and
the fetched remote-tracking branch all resolved to
`0b57776bf911faeea7e28ea133706fb03740ffeb`
(ahead/behind `0/0`). Cargo metadata resolved `symbolica`, `numerica`, and
`graphica` to this checkout, and the release dependency file contained
`vendor/symbolica/` paths with no `vendor/symbolica-src/` path.

That baseline contained the consolidated optimized polynomial/resultant source
in `df088bc` and the matching low-level Numerica kernels in `e029754`. Newer
`dev` commits through `89749e2` further revised polynomial GCD, factorization,
interpolation, packed products, and Hensel lifting. The maintained performance
gate measured that exact revision rather than inferring speed from commit
ancestry alone. Those observations are not fresh measurements of `fb845d34`.

The `dev` and former `dev_poly` histories were imported/squashed rather than
joined by literal Git ancestry. Comparing former `dev_poly` revision `76e3eb6`
with predecessor `dev` revision `bd0c137`, Git object IDs prove byte-identical
blobs for `src/poly/{resultant,gcd,polynomial,factor}.rs` and a byte-identical
complete `lib/numerica` tree. Historical `dev` at `0b57776` contained later source
changes as well as the complete resultant and power improvements:

- `4fd8443c` adds the public coefficient-domain `PolynomialResultant` trait,
  the primitive-integer rational adapter, and internal integer-Ducos/CRT
  selection. `RationalField` owns the specialized selector, `IntegerRing`
  explicitly opts into the generic default, and there is no overlapping
  blanket field implementation on stable Rust.
- `b8fa6b53` replaces linear rational-polynomial powering with exponentiation
  by squaring and delegates the coefficient-field implementation to it.

Both commits were ancestors of that historical upstream baseline. The earlier
HyperLica series added further rational and polynomial changes and standalone
diagnostics; it did not change the existing checked-in PolyBench results.
The root crate enables
Symbolica's `faster_alloc`, GMP-integer, and MPFR-float features; none of the
omitted features selects an alternative polynomial kernel.

## Fixes already present in the historical baseline

Four formerly local correctness workarounds were already part of the old
`0b57776` baseline, outside the two historical resultant/power artifacts:

- Integer `RationalPolynomial::from_num_den(..., true)` removes the common
  scalar content from numerator and denominator even when their active
  variables are disjoint. Symbolica's native
  `integer_content_is_removed_from_disjoint_variables` regression passed for
  the exact `21*(1+3*x^2+x^20) / (21*(1+y))` case.
- F4 uses a fixed pre-matrix basis when recording simplification rules. The fix
  is Symbolica commit `1f6621d`; both the
  native `prevent_live_basis_update` regression and Hyperbolica's minimized F4
  example passed. Hyperbolica therefore calls `GroebnerBasis::new` directly and
  no longer carries `ensure_groebner_basis` or its Buchberger fallback.
- `FactorizedRationalPolynomialField::is_one` includes `numer_coeff`, so a
  scalar such as `2` is no longer misclassified as one.
- Rational-polynomial differentiation constructs both quotient-rule terms
  with `do_gcd = true`, removing scalar content exposed by differentiation.
  Both fixes are in Symbolica commit `bd0c137`, so Hyperbolica no longer carries
  either workaround.

## Historical resultant/power artifacts

The three patch files retained beside this document are provenance artifacts
against predecessor `bd0c137e62cbd07ebc045bc4b04ac661a14f3be6`:

- `symbolica-dev-resultant.patch` contains only the resultant work;
- `symbolica-dev-rational-power.patch` contains only binary powering;
- `symbolica-dev.patch` is their combined historical patch.

They are deliberately not applied to the selected snapshot. See
[`SYMBOLICA_PATCH.md`](SYMBOLICA_PATCH.md) for exact hashes, historical scope,
upstream commit IDs, and predecessor application instructions. The earlier
three-commit `symbolica-hyperlica.patch` is also retained unchanged as history;
only `symbolica-hyperlica-dev-20260908.patch` belongs to the current recipe.

## Archived source copy

`vendor/symbolica-src` is the previous copied snapshot at
`61f88b381247f5f25dd7832b0286b5e362bd4148`. It remains only as an audit and
historical patch reference and is not selected by Cargo. Do not use its test
results or performance numbers as evidence for the current build.
