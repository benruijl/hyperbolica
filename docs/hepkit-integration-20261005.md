# HEPkit integration validation, 2026-10-05

The integration API is registered as `symbolica.community.hepkit.integration`
in the shared community wheel. Hyperbolica retains its Rust engine and reusable
bindings; Symbolica-integrate and existing IBP implementations are unchanged.
The standalone Hyperbolica Python wheel has been retired.

## Source and build boundaries

- Rust development: pristine Symbolica main `75f8350094b90254ee71dc2a391fde0d14b0204a`.
- Community kernel: existing `community` revision `942bd2c0cd2ef16414d69c176fc9eff7b21c0ab2`.
- Native toolchain: Rust 1.96.0. Python wheel: CPython 3.13, ABI3 floor 3.9.
- Development builds retain GMP/MPFR and the faster allocator; the community
  wheel retains its system allocator and resolves exactly one Symbolica,
  Numerica, Graphica, and PyO3 package.

## Native checks

- 469 Rust library tests and 16 integration tests passed in isolated processes.
- All 7 Python-binding Rust tests and 9 public Rust examples passed.
- All 49 independent Python benchmark-driver tests passed.
- All 5 installed-wheel integration contract tests passed, in restricted and
  licensed mode. The licensed run also passed all 83 selected HEPkit, one-loop,
  and integration tests, including diagram generation and interruption recovery.
- Generated integration/community typing checks and wheel/shared-kernel checks passed.
- Native Clippy, formatting, pure-Symbolica and module-size gates passed.

The host's expression pickle stores raw, process-local symbol IDs. Cross-process
persistence uses the existing `Expression.save`/`Expression.load` API, verified
with a different symbol-registration order. No expression serializer override
is installed by Hyperbolica.

## Partial-fraction relocation comparison

Both release binaries use Rust 1.96.0, opt-level 3, LTO, one codegen unit,
the same arithmetic features, and CPU affinity 383. Five alternating-order
samples compare parent `90b78212e1b0e1958a604082c6ad032aa856a39f` against this
implementation. Each process repeats its request (500 iterations for the wide
power and improper cases; 30 for parameter-leading poles). Exact JSON outputs
match between versions and across iterations. Timings include the compatibility
adapter and initial request; these are short warm-process checks, not a new
HyperFLINT benchmark suite or a claim of whole-program performance parity.

| Case | Before, µs/iteration | After, µs/iteration | Change |
|---|---:|---:|---:|
| wide_power_32 | 105.302 | 106.280 | +0.93% |
| parameter_leading_poles | 326.267 | 327.233 | +0.30% |
| improper_fallback | 156.482 | 154.822 | -1.06% |

The observed median differences are approximately ±1%. Raw samples are in
[the CSV](benchmark-evidence/hepkit-integration-apart-20261005.csv).
Before executable SHA-256: `a9471de436331174702d4b2442f9640865398bf8e868f9ae68f3ade2b6e96873`.
After executable SHA-256: `c4899d7edc55e5bbe0bf5e9d998ede81aa4bf4a02f653f29dad22c39cc18723b`.
