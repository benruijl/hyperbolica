# HyperLica's local Symbolica patch series

The active dependency is `vendor/symbolica`, a Git checkout ignored by the
parent repository. It now contains the audited generic improvements and bug
fixes; Cargo no longer depends on an audit directory under `target/`.
The historical `vendor/symbolica-src` snapshot is unchanged and is not built.

The complete new work is preserved in the tracked
[`symbolica-hyperlica.patch`](symbolica-hyperlica.patch), including all new
source modules, tests, examples and documentation. No remote push or PR has
been made for these three commits. They are ready for further upstream review;
the intended eventual reviewer is Ben Ruijl.

## Source identities

| Item | Identity |
|---|---|
| Official upstream base | `0b57776bf911faeea7e28ea133706fb03740ffeb` |
| Rational kernels, arithmetic and constant contexts | `c4433a33bbcae1639f4c391700f37278875c8da9` |
| Factored normalization and native partial fractions | `d016b123a4c7d463d10e3b56279e2fb66a41f75e` |
| Bug reproducers and benchmark oracles (local tip) | `e33a1fb70abe70e774dd6a55baa5a8e1f42cc8e3` |
| Complete patched Git tree | `49dfd8a2b3e16c21ef78425c945578ce8a7815fd` |
| Mail patch SHA-256 | `7cb664d6287ce31d13681b75c0bd4f3ec512d95d90ba8624e1c5af397ad35133` |

The local vendor was updated by fetching the audit clone and fast-forwarding
its clean checkout. Its official `origin` remote is unchanged.

For a fresh parent checkout, follow the commands in
[`SYMBOLICA_SNAPSHOT.md`](SYMBOLICA_SNAPSHOT.md): clone the official repository,
select the exact base, and apply the mail patch with `git am`. This creates
three local commits. Their IDs may differ with committer metadata; the complete
tree must be identical. `scripts/check-pure-symbolica.sh` enforces that tree,
the official origin, required upstream ancestry and a clean working tree.
Never apply the historical resultant/power patches as well: those changes are
already in the base.

## Standalone bug reproducers

Detailed expected/broken behavior and exact commands are in
[`symbolica/benches/bug-reproducers.md`](symbolica/benches/bug-reproducers.md).
All examples use original public APIs and can be copied to an unpatched
checkout without the new library code. Set `SYMBOLICA_LICENSE` in your own
environment if needed; no license value is stored in this patch.

| Example in `vendor/symbolica/examples/` | Status |
|---|---|
| `mre_factored_construction.rs` | Eight fixed constructor/arithmetic groups; ten individual baseline negative controls fail as expected. |
| `mre_constant_rearrangement.rs` | Fixed empty-map growth/shrink for scalar polynomials. |
| `mre_sparse_partial_fractions.rs` | Fixed spurious zero pole order and lost variable context. |
| `mre_galois_upgrade.rs` | Unresolved intermittent factorization panic; bounded repeated attempts. |
| `mre_root_interval_expectation.rs` | Unresolved existing test expectation mismatch; refined root interval agrees. Not evidence of an incorrect root. |

The constructor, context and sparse-partial-fraction examples pass on patched
source and fail on pre-audit vendor builds. Both unresolved examples reproduce
on old and patched artifacts; they intentionally retain their failing checks.
The existing, previously upstreamed F4 reproducer remains documented in
[`SYMBOLICA_F4_MRE.md`](SYMBOLICA_F4_MRE.md).

## Validation boundary

Numerica passed 150 GMP/MPFR tests and 127 alternate-backend Malachite/Astro
tests. The frozen stage-4 Symbolica gate passed 625/627; the final combined
source, including duplicate-base regressions, passed 629/630 in a full-library
rerun. The remaining deterministic failure is the interval expectation. The
intermittent factorization test happened to pass that suite run, but its
standalone MRE still panics. This is not a fully green upstream test suite.

The current vendored port, including the duplicate-base fix, passed all
449 library tests and the remaining all-target tests/smokes, 79 debug
differential cases, 47 benchmark-evidence tests and 15 matrix-harness tests.
Formatting, module-size and the pure-Symbolica source/dependency gate passed.
The current vendored-build validation and precise historical boundaries are recorded in
[`../docs/parity-progress-20260907.md`](../docs/parity-progress-20260907.md).
No new combined-patch end-to-end timing is claimed by the packaging operation.

Historical raw timing CSVs, logs, binaries and source archives referenced by
the audit reports live under the ignored `target/parity-audit-20260907/`
directory on the audit workstation. They are not bundled with a fresh clone.
The checked-in manifests, benchmark harnesses, source identities and native
examples permit fresh measurements, not retrieval of those historical files.
