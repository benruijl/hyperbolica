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

The tracked snapshot has one post-snapshot optimization used by this crate:

- `src/domains/rational_polynomial.rs` implements rational-polynomial powers by
  exponentiation by squaring, and `RationalPolynomialField::pow` delegates to
  that implementation. Focused coverage lives in
  `tests/rational_polynomial.rs`.

Keep subsequent changes to the vendored source minimal, covered by Symbolica's
own tests, and listed here. The ignored original checkout remains unchanged.
