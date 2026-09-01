# Upstream baseline and scope

The compatibility baseline is SubTropica commit
`adfd3af3be234cb43a2322bd9ec442caa26edd74` on `main`, inspected on
2026-08-31. Differential and performance results must record a newer commit if
the oracle checkout advances.

## Mathematical implementation in scope

The Rust port tracks the behavior implemented by these upstream areas:

| Upstream area | Rust area | Required behavior |
| --- | --- | --- |
| `core/poly`, `core/rat`, `core/factored_rat` | `core` | exact arithmetic, factorization, resultants, substitution, derivatives, canonical forms |
| scalar/split/period/ZW helpers | `core` | canonical coefficient representation, context-aware caches, deterministic reconstruction |
| `symbols` | `symbols` | words, Hlogs, MPLs, canonical content keys |
| `algebra` | `algebra` | shuffles, conversion, differentiation, linear factors, partial fractions, algebraic letters |
| `algebra/euler_*` | `algebra/euler` | finite-field Euler counts and conservative genuine-letter filtering, using Symbolica Gröbner bases in-process |
| `series` | `series` | Laurent, Hlog, and MPL expansions with exact coefficients |
| `reduce` | `reduce` | contour breakup, MZV expansion/reduction, periods, fibration/zero tests |
| `integrator/lr_search.cpp::verify_order_is_lr` and `test/unit/test_verify_order_multigroup.cpp` | `integrator/lr_verify` | prescribed-order certification, fast single-path accept, intersection-refined rejection, malformed and blocking diagnostics |
| remaining `integrator` | `integrator` | transform, primitive, regularization, integration steps, full driver, LR scan/search |
| stable C ABI | `c_abi` | schema 2 entry points, in-band JSON errors, matching ownership and version contracts |
| JSON CLI | `bridge` and `bin/hyperflint` | migration/oracle protocol, deterministic mathematical fields |

The implementation language and CAS backend are intentionally different. A
line-for-line translation is neither expected nor desirable when a public
Symbolica operation already provides the same algebraic primitive.

## Replaced or excluded mechanisms

The following upstream mechanisms are not dependencies of the Rust library:

- FLINT storage, factorization, GCD, and rational-function code are replaced by
  Symbolica domains.
- The external `msolve` Euler protocol is replaced by Symbolica's public
  finite-field Gröbner-basis API.
- OpenMP and C++ allocator machinery are replaced by Rust ownership, Rayon at
  coarse independent-entry boundaries, and Symbolica's configured allocator.
- Mathematica LibraryLink is not linked into the mathematical Rust kernel.
  The optional, separately built `librarylink/` adapter implements the pinned
  SubTropica loader surface by dynamically delegating to the stable C ABI; it
  needs neither FLINT nor a Wolfram runtime at build time. The stable C ABI and
  JSON adapter remain the language-neutral integration points.
- Diagnostic probes whose only purpose is inspecting a C++/FLINT allocator,
  OpenMP runtime, or C++ hash-table implementation are not compatibility
  requirements. Equivalent Rust performance telemetry may be added where it
  answers a current bottleneck.

Excluding an implementation-specific probe does not exclude the property it
was protecting. Determinism, cache bounds, memory ownership, canonical output,
and thread safety remain testable requirements.

## Upstream regression corpus to mirror

The port should maintain direct Rust coverage for each portable upstream test
class:

- algebraic-factor and partial-fraction characterization/reconstruction
- exact rational addition/content/canonicalization equivalence
- factor-table and LR order/scan/verify behavior, including multi-group input
- Euler count and filter conservatism
- MZV, period tuple, contour, and chained-reduction data files
- parser unary precedence at the compatibility boundary
- full-driver serial/parallel determinism
- stable C ABI symbol whitelist, ownership, errors, and CLI byte identity
- the Smirnov integration fixtures
- construction/cache behavior that is independent of the C++ container type

Allocator-specific and sanitizer-specific upstream tests are replaced by Rust
memory-safety tooling and boundary ownership tests rather than copied verbatim.

## Completion evidence

A feature is not considered ported because a similarly named Rust function
exists. Completion requires:

1. a source-level mapping to the upstream behavior;
2. native Symbolica API review for every general CAS operation;
3. unit/property tests for mathematical invariants and errors;
4. differential evidence for the compatibility surface;
5. release-mode timing evidence for hot or end-to-end paths; and
6. an explicit note for any intentional output normalization difference.

The current mapping decisions for Symbolica are in
`docs/symbolica-api-audit.md`; compatibility fixture policy is in
`tests/COMPATIBILITY.md`.

The pinned source's public header/implementation exposes eight current `hf_*`
symbols, although its older containment-only golden file omits the later
factor-table and LR-scan entries. Hyperbolica pins the exact eight-symbol
header contract; the discrepancy and verification procedure are documented in
`docs/verification.md`.

### Prescribed-order LR verification

The verifier mirrors upstream `lr_search.cpp` at baseline `adfd3af`: it first
walks the requested order with a per-request Symbolica-backed reduction engine.
That path is a sound fast accept because it over-approximates the refined
letter sets. A provisional blocker triggers the same subset construction as
the search: each subset/group keeps only the proportional intersection of all
one-pivot parent reductions. The requested prefixes are then checked against
that authoritative table. This preserves the e26 multi-group fix where the
single path contains the spurious letter `x8^2 + x8 + 1`.

`carry_discharge` is intentionally not part of the typed verifier. With
algebraic letters enabled, terminal quadratics are admitted, but a quadratic
depending on a still-pending integration variable is rejected and reported via
`forbidden_dep`. The JSON adapter still emits the request-gated zero-valued
carry profile and the same strategy/inert-search envelope as HyperFLINT.
