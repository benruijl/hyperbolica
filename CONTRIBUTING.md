# Contributing

Hyperbolica is a pure-Symbolica port. Correctness and performance changes must
preserve the representation and verification rules below.

## Before implementing algebra

For every operation that could reasonably be a CAS primitive:

1. Search Symbolica's public exports and prelude.
2. Search the relevant domain implementation and its tests/examples.
3. Search the Python/public wrapper surface as a cross-check for differently
   named functionality.
4. Record the chosen API, source anchor, and retain/wrap/replace decision in
   `docs/symbolica-api-audit.md`.

Only domain-specific hyperlogarithm behavior or a measured adapter mismatch is
a reason to keep custom algebra. The factored Taylor partial-fraction adapter is maintained locally because the
shared community kernel does not export that specialization. It still delegates
all polynomial arithmetic and general decomposition to Symbolica.

Do not duplicate a general polynomial,
rational-function, factorization, resultant, series, root, or Gröbner-basis
algorithm without an explicit audit entry and benchmark evidence.

## Expression rules

- The production Rust boundary accepts Symbolica `Atom` values.
- Do not introduce a string parse/format round trip in the Atom API or kernel.
- Declare every Hyperbolica-owned head in the single `initialize!` block in
  `src/symbols.rs`.
- Represent indexed special objects as calls of a registered head. Never mint
  names such as `Wm_17` or `mzv_m2_3` dynamically.
- Compatibility strings belong in `convert`, `bridge`, or explicit legacy
  adapters and must convert to the registered structural form immediately.

## Source organization

Give each file one semantic responsibility. Prefer a private submodule and a
small re-exporting `mod.rs` when implementation, state, conversion, and tests
start competing for space. New production files should normally stay below
600 lines; an existing larger file should shrink when materially extended.

Do not place mathematical logic in Python, C ABI, or JSON handlers. Those
layers validate/convert data and delegate to the typed API.

## Required checks

At minimum, a compile-verified change runs:

```sh
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets --no-run
cargo check --features python
cargo check --features python_stubgen
scripts/check-pure-symbolica.sh
scripts/check-module-size.sh
```

On a machine where the Symbolica runtime is available, run the serialized test
suite as well:

```sh
scripts/test-unlicensed.sh
```

Add unit tests for success, zero/identity, invalid input, and the relevant
canonicalization/error invariant. Add reconstruction or property-style tests
when output order is not mathematically meaningful. Public behavior also needs
an Atom-level integration test and, where applicable, Python/C/JSON boundary
coverage.

## Performance changes

Benchmark release builds on identical inputs, hardware, affinity, and thread
counts. Verify exact output before timing. Report medians, individual samples,
both executable revisions, toolchain, CPU, and gate thresholds. A compile-only
check or a microbenchmark of a different workload is not parity evidence.

Use the external C++ HyperFLINT executable only from differential and benchmark
harnesses. It must never become a dependency or subprocess of production code.

## Milestone commits

Commit a milestone only when its declared verification gates pass. If runtime
execution is blocked by the Symbolica singleton or another external condition,
say so in the commit/handoff and describe the milestone as compile-verified,
not runtime-verified. Keep unrelated changes from the original local Symbolica
worktree out of this repository except through the documented vendored
snapshot.
