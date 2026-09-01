# Symbolica source snapshot

`symbolica-src/` is the exact source used to build Hyperbolica. It was copied
from the local Symbolica `dev` worktree requested for this port, including its
improved polynomial/resultant implementation.

- Source commit: `61f88b381247f5f25dd7832b0286b5e362bd4148`
- Source branch state at snapshot time: 160 commits ahead of `origin/dev`
- Additional working-tree change: `src/poly/gcd.rs`
- SHA-256 of that binary Git diff: `d45cb4e7a87b6e581b6f8a3193183c341029d260801451238a05c4ed6e8ea973`
- Snapshot date: 2026-08-31

The original nested checkout is intentionally excluded from the parent Git
repository. Its `.git`, `target`, `.venv`, and benchmark-result directories are
not needed to reproduce the crate and are not part of this snapshot.

Do not replace this directory with a crates.io Symbolica release or a submodule
without first proving that the improved polynomial kernels are present and
rerunning the compatibility and performance gates.

## Hyperbolica-maintained patch

The tracked snapshot has four post-snapshot changes used by this crate:

- `src/domains/rational_polynomial.rs` implements rational-polynomial powers by
  exponentiation by squaring, and `RationalPolynomialField::pow` delegates to
  that implementation. Focused coverage lives in
  `tests/rational_polynomial.rs`.
- `src/domains/factorized_rational_polynomial.rs` makes
  `FactorizedRationalPolynomialField::is_one` check `numer_coeff` as well as
  the numerator, denominator factors, and `denom_coeff`. Without that check a
  scalar such as `2` was misclassified as one by the field trait even though
  the value's inherent `is_one` correctly rejected it. A focused regression
  lives beside the implementation.
- `src/domains/rational_polynomial.rs` removes common scalar content after the
  integer polynomial-GCD step. Some GCD strategies return a primitive
  associate, so without this final normalization mathematically equal rational
  functions could retain a shared integer scale and compare or hash
  differently. A focused regression lives in `tests/rational_polynomial.rs`;
  `SYMBOLICA_PATCH.md` records the rationale, complete patch, and refresh steps.
- `src/poly/groebner.rs` fixes F4 simplification-rule registration by taking a
  snapshot of the pre-matrix basis. Rows created by one Macaulay matrix can no
  longer be registered as reducers for later rows from that same matrix, which
  previously erased an unprocessed S-pair. The defect was introduced by
  Symbolica commit `a49b86ac364c684281a594bc88cfc7022766746c` and remains in
  public `dev` at `f9f756250201a13d2b06a5a75b383cb1e522c69f`. Focused and
  deterministic randomized regressions live in the same source file.
  `ensure_groebner_basis` and its Buchberger completion remain as a defensive
  exact-verification layer for Hyperbolica's reduced Euler bases;
  `SYMBOLICA_PATCH.md` records both changes and their verification commands.

Keep subsequent changes to the vendored source minimal, covered by Symbolica's
own tests, and listed here. The ignored original checkout remains unchanged.
