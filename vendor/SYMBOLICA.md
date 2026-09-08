# HyperLica's selected Symbolica source

The active dependency is `vendor/symbolica`, an official Git checkout ignored
by the parent repository. The 2026-09-08 refresh selects the fetched upstream
`dev` base below, the cumulative local patch, and the scratch-recycling follow-up. Cargo does not
build either an audit directory under `target/` or the historical
`vendor/symbolica-src` snapshot.

## Current source identities

| Item | Identity |
|---|---|
| Official upstream base | `fb845d34bda8ccf1fedef6544d3aa46dc24944e3` |
| Upstream base tree | `d232cb41a72ec749f1e1ce23b0088a307deca45f` |
| Active vendor tip | `1a01bdbc166c040d176584a7c9ecd1810e73537a` |
| Scalar-only constructor correction | `6e20db0336fbaa2d84df22ce6ca8ff5ac259f072` |
| Cumulative reconstruction commit | `df7eb980ecd3040ac0fd0bacc37b0f34d74b4701` |
| Complete selected Git tree | `d8925723c37ec35a80a40c6b432b565d747ac29f` |
| Cumulative base mail patch | [`symbolica-hyperlica-dev-20260908.patch`](symbolica-hyperlica-dev-20260908.patch) |
| Cumulative patch SHA-256 | `7c76d4741d8eb3c2e6b75837129c898edb974857694163a2605f42fb0392a3e5` |
| Earlier scalar-corrected release SHA-256 | `cf72945dcada1632e31dca72e883ad91997d741e755b8050af335a16963ffea4` |
| Scratch follow-up patch | [symbolica-heap-scratch-20260908.patch](symbolica-heap-scratch-20260908.patch) |
| Follow-up SHA-256 | `e8ceb492594793354a5c811a24747f2fec9237e7b35c918247eee6550494aa0d` |

Upstream commit `bd29c4704eaa4ea5754330a7c43d59d2bd50e065` supplies the powered
denominator, leading-unit, rational-scaling, duplicate-factor, zero-denominator,
constant-variable-map and sparse-partial-fraction corrections. It also
normalizes univariate GCD results with a zero operand, including algebraic
extensions, addressing the previously observed Galois-upgrade assertion.
The local patch uses those upstream implementations rather than replacing
them with the former local constructor implementation.

The cumulative patch retains generic fraction arithmetic and polynomial kernels,
rational exact division, equal-denominator rational addition, differentiation
when the denominator is independent of the target variable, native
factored-coefficient partial fractions, checked multiplicities, and regression
tests. It also corrects scalar-only factored construction: integer constants
are left to scalar normalization instead of entering polynomial factorization
with a zero-width variable map.
These are source changes, not a claim of measured performance for the new build.

For a fresh checkout, use [`SYMBOLICA_SNAPSHOT.md`](SYMBOLICA_SNAPSHOT.md):
apply the cumulative development mail patch, then the scratch-recycling mail
patch, with `git am`. Commit IDs can differ due to metadata; the complete
selected tree must match. Run `scripts/check-pure-symbolica.sh` to check it.
Do not additionally apply the older scalar, three-commit, or resultant patches.

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

The current upstream `License.md` is titled "Symbolica Source-Available
License 1.0" and differs from the old snapshot's notice. Its SHA-256 is
`95576170a17bc8996fb8a94473745545048b6edc5d91e3867f091f7fc6b26981`.
Refer to the complete bundled notice and the parent distribution notice;
the package version alone does not identify either source or license content.

## Retained regressions and scalar reproducer

Exact commands and historical behavior are in
[`symbolica/benches/bug-reproducers.md`](symbolica/benches/bug-reproducers.md).
The retained scalar example uses original public APIs and can be copied to
older checkouts.
Supply `SYMBOLICA_LICENSE` through the environment if needed; no license value
is stored in the patch or evidence.

`examples/mre_factored_scalar.rs` constructs exact `1/2` over Q with no variables
or supplied denominator factors, then checks the canonical integer-backed
representation. Run it with:

```sh
cargo run --manifest-path vendor/symbolica/Cargo.toml \
  --example mre_factored_scalar
```

The standalone Symbolica workspace does not track `Cargo.lock`; this first run
resolves it. Use `--locked` for subsequent runs with that lock, or with the
exact archived validation lock. HyperLica builds use the separately committed
root `Cargo.lock` and remain locked.

Five obsolete standalone MRE files were removed in cleanup commit `15bd1f6`:
the factored-construction, constant-rearrangement, sparse-partial-fraction,
Galois-upgrade and interval-expectation examples. Their original sources remain
in the historical patch/Git history. The native unit tests remain, including
exact constructor arithmetic, constant maps, sparse pole orders, monic GCD
with zero operands, `galois_upgrade` and root isolation. No production source
or library test was removed by the cleanup; tracked `src`, `lib` and
`Cargo.toml` are byte-identical to the tested scalar-fixed commit `6e20db0`.
The standalone native test lock is an ignored validation artifact, not part
of the selected Git tree; its hash is recorded with the native test evidence.

The current root-isolation unit test expects `[3/16, 9/32]`, not the historical
`[15/64, 9/32]`; its refined interval remains `[1023/4096, 2049/8192]`.
The expectation correction is not evidence of an incorrect root. Finite
successful randomized sampling is not a proof that every Galois path succeeds.
The earlier F4 reproducer remains documented in
[`SYMBOLICA_F4_MRE.md`](SYMBOLICA_F4_MRE.md).

## Validation boundary

Pristine upstream passed 604/604 native library tests and its standalone MRE
checks, including 300 bounded Galois-upgrade attempts. The corrected local
implementation at `6e20db0` passed 637/637 native library tests, 25 focused tests
and 17 standalone regression invocations, including the new scalar example and
100 Galois attempts. Those standalone checks preceded the obsolete-example
cleanup; they do not imply that all of those files remain in the final package.
The complete final library source is identical to that tested implementation.

The scalar-corrected port passed all 449 library tests, its remaining
all-target checks, and all 79 C++ differential cases in both debug and release
builds (24 use declared normalization). The `--all-targets --all-features` check
also passed. Package-scoped Hyperbolica formatting passed. Workspace-wide
formatting reports a pre-existing line-wrap discrepancy in upstream
`src/transcendental.rs:4873` under Rustfmt 1.89; it was not changed during the
release build.

Corrected release compilation completed. Its fresh 162-case matrix passed all
exact comparisons and repeatability checks; all 162 paired runtime ratios and
all 486 individual timing-pair ratios favored Rust in this run. The overall
geometric-mean Rust/C++ ratio is 0.412, using three adjacent pairs per case on
CPU145. Peak RSS was higher for Rust in 55 cases, including roughly twice C++
for `tst1`; runtime gains do not establish memory parity. Detailed times,
measurement limits and the separate heavy-case coverage are in
[`performance-dev-20260908.md`](../docs/performance-dev-20260908.md).

The preliminary `7094dda` measurement remains preserved separately as
`pre-scalar-fix-vs-cpp`. It is not relabeled as corrected-build evidence; the
final corrected run is `final-vs-cpp`. Neither run establishes universal parity.

For historical context only, the previous local tip `e33a1fb` passed 150
Numerica GMP/MPFR and 127 Malachite/Astro tests. Its final full Symbolica rerun
passed 629/630; the deterministic interval expectation failed, and the separate
randomized Galois MRE still reproduced an assertion. The earlier frozen stage-4
run passed 625/627. The previous vendored port passed 449 library tests, its
remaining all-target checks, 79 debug differential cases, 47 benchmark-evidence
tests and 15 matrix-harness tests. These are historical results only.

New evidence belongs under the ignored `target/performance-dev-20260908/`;
older raw results, binaries and archives remain under
`target/parity-audit-20260907/`. These workstation artifacts are not bundled
with a fresh clone. The checked-in manifests and harnesses permit new runs,
not retrieval of historical files. The fresh timing conclusions apply only to
the measured workload corpus, not to unselected or preprocessing-dependent inputs.

The final `symbolica-final-source.tar` archive has SHA-256
`2da5a6bc1d2ea3a6a61479f009df050105c9a2d9961f6c78ac6ed7ce4d009573`.
Earlier `symbolica-source.tar` and `hyperlica-source.tar` archives belong to
the pre-scalar-fix measurement and remain preserved separately.

## Preserved historical patch

[`symbolica-hyperlica.patch`](symbolica-hyperlica.patch) remains unchanged as
the earlier three-commit series on upstream base
`0b57776bf911faeea7e28ea133706fb03740ffeb`, ending at
`e33a1fb70abe70e774dd6a55baa5a8e1f42cc8e3` with tree
`49dfd8a2b3e16c21ef78425c945578ce8a7815fd`. Its SHA-256 is
`7cb664d6287ce31d13681b75c0bd4f3ec512d95d90ba8624e1c5af397ad35133`.
It is not applied to the new base. See [`SYMBOLICA_PATCH.md`](SYMBOLICA_PATCH.md)
for the remaining historical patch records.
