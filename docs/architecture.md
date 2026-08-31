# Architecture

Hyperbolica has one mathematical kernel and several deliberately thin
boundaries. The kernel operates on typed Symbolica-backed values; parsing,
serialization, Python conversion, and C layout are adapters around it.

## Dependency direction

The intended dependency flow is:

```text
Symbolica
   |
core + symbols
   |
algebra + series + reduce
   |
integrator
   |
typed Atom API
   |
Python / C ABI / compatibility bridge
```

Lower layers must not call a transport layer. In particular, the Atom API must
not pass through JSON or the compatibility parser, and the mathematical kernel
must not know about PyO3 objects.

## Representations

### Expression boundary

`api::prepare_atom` accepts a Symbolica `Atom` and an ordered list of
integration `Symbol`s. It separates rational factors, recognized Hyperbolica
functions, and residual function indeterminates without converting the whole
expression to text. `PreparedAtomInput` owns this work so repeated integrations
do not rediscover it.

The compatibility parser remains available only for the upstream CLI and
fixture protocol. It is not a second production representation.

### Polynomial context

`core::PolyCtx` fixes an ordered set of Symbolica indeterminates. Stable indices
are required by the upstream algorithms and make caches deterministic. A
context can contain ordinary symbols and registered function atoms; unknown
indeterminates are rejected instead of silently changing the ring.

`core::Poly` and `core::Rat` wrap Symbolica polynomial and rational-polynomial
objects. Algebraic operations should use their native Symbolica storage. Any
HyperFLINT-shaped numerator/denominator view is a compatibility projection and
must be lazy so reads do not impose conversion cost on every hot operation.

### Symbolic coefficients

`core::SymCoef` represents the small commutative coefficient vocabulary needed
by period reduction: rational functions, powers of `Pi` and `I`, logarithms,
contour deltas, and opaque period factors. It is intentionally narrower than a
general CAS expression. Conversion back to an `Atom` uses Symbolica built-ins
and the central Hyperbolica symbol registry.

### Hyperlogarithmic objects

Words contain rational-function letters. Hlogs, MPLs, shuffle products,
regulators, and period products retain typed structures while algorithms are
running. Collection keys must be canonical and deterministic; formatting is
never a semantic equality test.

## Symbol registry invariant

All library-owned Symbolica heads are declared together by the `initialize!`
block in `src/symbols.rs` and retrieved through `symbols::heads()`.
Algorithms must not call `symbol!` to mint a special object.

Parameterized objects use function atoms of these heads:

```text
hyperbolica::MZV(-2, 3)
hyperbolica::Wm(1)
hyperbolica::Wp(1)
hyperbolica::sqrt_disc(1)
hyperbolica::Period(7)
hyperbolica::delta(x)
```

This keeps the set of registered symbols finite, allows structural matching,
and gives Python and Rust the same representation.

## Module responsibilities

### `core`

- context construction and interning
- native exact polynomials and rational polynomials
- factor-preserving rational products
- canonical signatures and split caches
- exact restricted symbolic coefficients

### `symbols`

- central Symbolica head registration
- words and word lists
- typed Hlog/MPL containers

### `algebra`

- shuffle and conversion identities
- rational differentiation and factor extraction
- native partial fractions
- algebraic-letter metadata and Vieta reduction
- pure-Symbolica Euler-characteristic/Groebner calculations

### `series` and `reduce`

- exact series recurrences used by endpoint analysis
- contour breakup and positive-axis closure
- MZV expansion and table-driven reduction
- period evaluation and zero-function/fibration tests

These modules retain domain algorithms when Symbolica has no equivalent. Any
general CAS operation is first checked against the pinned Symbolica public API;
the decisions and source anchors live in `symbolica-api-audit.md`.

### `integrator`

- transform Hlogs/MPLs into an integration-variable basis
- build primitives and regularize zero/infinity boundaries
- drive multi-variable integration
- scan and search linearly reducible variable orders

Independent entries may run in parallel, but their collection order and final
normal form must be deterministic.

### Boundaries

`api` is the canonical Rust boundary. `python` wraps it with native Symbolica
Python expressions and releases the GIL around computation. `c_abi` owns all C
allocation/error conventions. `bridge` owns the legacy JSON operation table and
string parsing. None of them reimplements mathematical logic.

## Purity and external oracles

The production dependency graph contains only Symbolica as a CAS. The pure
gate checks dependencies, imports, and forbidden solver subprocesses. External
C++ HyperFLINT is permitted only in test/benchmark shell harnesses, where it is
an independently executed oracle.

Vendored Symbolica is a reproducible source snapshot, including the improved
resultant implementation requested for this port. Changes needed by
Hyperbolica should be made in that tracked snapshot and documented, never in
the ignored original checkout.

## Verification layers

1. Unit tests cover exact identities, invariants, error paths, and canonical
   ordering inside each semantic module.
2. Atom integration tests exercise the production boundary without parsing.
3. JSON differential fixtures compare every compatibility operation with C++
   HyperFLINT.
4. Property-style reconstruction tests validate transformations such as
   factorization and partial fractions independently of output ordering.
5. Criterion isolates hot kernels; the JSON benchmark harness measures the
   same end-to-end cases and enforces the release slowdown threshold.
6. Python and C smoke tests verify ownership, exceptions, expression identity,
   and ABI behavior without duplicating core assertions.

Compilation-only checks are useful but not evidence of runtime correctness or
performance. If Symbolica runtime execution is unavailable, that limitation is
recorded and the affected milestone remains unverified.

## File-size rule

Source files should remain small enough to have one obvious responsibility.
When a semantic module grows, split implementation concerns into private
submodules and re-export the stable public surface from `mod.rs`. Tests should
live beside the behavior they specify or in a focused integration-test file;
large catch-all test modules are avoided.

`scripts/check-module-size.sh` enforces a 600-line ceiling for every Rust
source file. The ceiling is a ratchet, not a target: new modules should be
smaller, and a file approaching the limit should be split by responsibility
before more behavior is added.
