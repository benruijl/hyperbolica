# Verification and performance evidence

Hyperbolica separates CAS-independent gates from checks that acquire a
Symbolica runtime. This keeps ABI and fixture regressions visible in ordinary
CI while making it impossible to mistake a successful compile for mathematical
or performance evidence.

## CAS-independent gates

- `scripts/lint-fixtures.sh` validates both JSONL corpora before a binary is
  built or run. The checked-in JSON Schemas document the formats; the executable
  lint additionally enforces unique canonical requests and an operation-specific
  allowlist of normalization fields.
- The CAS-independent subset,
  `cargo test --test c_abi_contract -- --skip successful_math_operations_cross_the_abi_with_owned_envelopes`,
  exercises NULL input, malformed UTF-8 and JSON, caller-owned allocation, the
  NULL-safe deallocator, and the borrowed four-component version pointer. None
  of those selected requests reaches CAS dispatch.
- `scripts/check-abi.sh` enforces exact equality between the shared-library
  `hf_*` exports, the public header, and `tests/abi/symbols_golden.txt`. It also
  compiles C and C++ consumers, links a C harness, and by default runs only its
  NULL-input transport/ownership paths. Set `ABI_RUN_SMOKE=0` when cross
  compiling.

The pinned C++ baseline has a historical mismatch: its current `c_abi.h` and
implementation contain `hf_factor_table` and `hf_find_lr_orders_scan`, while
its older containment-style golden file lists only six symbols. Hyperbolica
uses the current header/implementation contract requested for this port: six
operations plus `hf_free_string` and `hf_version_string`, exactly eight public
`hf_*` exports. Extra exports fail the gate as readily as missing exports.

## Symbolica-backed ABI gate

With `SYMBOLICA_LICENSE` set, `cargo test --test c_abi_contract` also invokes
valid requests through the exported functions. It mirrors the upstream
partial-fraction, linear-factor, and one-variable LR fixtures and adds the
convergent integral `int_0^infinity dx/(1+x)^2 = 1`. The test pins exact
UTF-8 JSON bytes for deterministic algebra responses, verifies the mathematical
LR and integration fields, rejects duplicate/missing envelope fields, proves
simultaneously live results are distinct writable allocations, and releases
every result through `hf_free_string`.

## Differential and benchmark runs

Both runtime scripts lint fixtures first. `scripts/differential.sh` then runs
the Rust binary and the independently built C++ executable; no C++ or FLINT code
is linked into the Rust crate.

`scripts/benchmark-compare.sh` refuses to time a workload unless both
backends first return the fixture-selected byte-exact, normalized, or semantic
successful response. Semantic comparison is deliberately field-scoped: each
declared algebraic string is parsed by the Rust/Symbolica expression parser in
the request's explicit variable context before the two canonical Atoms are
compared. The remaining response envelope, including regulator keys and other
structural fields, still follows the fixture's exact normalization policy.
Permutation fields are validated before normalization.

The semantic parser runs only during cross-backend preflight. Each backend's
preflight transport response is also captured separately, and every warmup and
measured response must equal that backend-specific snapshot. Parser startup is
therefore outside the timed process and harmless backend-specific formatting
cannot mask nondeterministic output.

Measured invocations are adjacent Rust/C++ pairs. The first backend alternates
within each workload, workload order is deterministically shuffled between
rounds, and the monotonic interval includes process startup. A CAS-independent
helper uses `perf_counter_ns` plus POSIX `wait4` to record elapsed, user,
system, and peak-RSS values. The child starts with an empty environment
containing only the four recorded thread/banner settings. For Rust invocations
only, when `SYMBOLICA_LICENSE` is set, the helper selectively inherits that one
named variable so licensed measurements can still use the sanitized
environment. The external C++ oracle never receives the credential, and the
value is neither placed in argv nor written to any evidence artifact. Metadata
records only two booleans describing credential inheritance, never the value.

Each run writes:

- `summary.csv`, retaining the five-column human-readable contract;
- `samples.csv` and `samples.json`, preserving paired execution order and
  raw wall/CPU/RSS measurements;
- `correctness.json`, binding every workload to its canonical preflight
  response hash;
- `analysis.json`, with per-workload paired geometric means, deterministic
  one-sided 95% bootstrap bounds, equal-weight global results, and RSS gates;
- `rust-build.json` and `cpp-build.json`, binding source revision, reopened
  binary hash, profile, command, and relevant build configuration; and
- `metadata.json` plus `qualification.json`, binding all artifacts to the
  exact policy, corpus manifest, fixture bytes, host, affinity, and binaries.

By default evidence goes under `target/benchmark-results/<run-id>/`. Existing
`RESULTS_FILE` users keep the same summary CSV format; sidecars go to
`$RESULTS_FILE.evidence` unless `EVIDENCE_DIR` is supplied. Exploratory runs
may use `RUST_REVISION` and `CPP_REVISION` to attribute externally built
binaries; qualification rejects both overrides.

Exploratory mode is the safe default. Its shorter sample count and configurable
limits are useful during development, but its policy result always has
`qualified: false`. The publication gate is:

```sh
BENCHMARK_MODE=qualification \
  HYPERFLINT_CPP=/absolute/path/to/release-portable/hyperflint \
  scripts/benchmark-compare.sh
```

The locked policy currently requires 12 matched pairs per workload, two
warmups, one thread, CPU 0 affinity, a 10,000-sample stratified bootstrap, a
global Rust/C++ upper confidence bound no greater than 1.05, no workload
geometric mean greater than 1.15, and peak RSS no greater than 1.25 times the
C++ oracle. It also requires a clean Rust LTO release build and a clean C++
`release-portable` build at the pinned upstream revision. Qualification
settings cannot be relaxed with environment overrides. Rust is built into a
new target directory after rejecting Cargo profile, flags, wrapper, target,
ambient-config, and native-toolchain overrides. The C++ path's adjacent cache locates the source
checkout only: the driver verifies its clean pinned revision, creates a fresh
CMake tree with locked portable flags, clean-builds `hyperflint-cli`, and
checks the compiled `HF_BUILD_VARIANT` stamp before timing it.

A performance claim is valid only when `qualification.json` says both
`status: "pass"` and `qualified: true`. A passed exploratory analysis,
compile-only result, manually edited status field, incomplete corpus, dirty
revision, mismatched artifact hash, unfair C++ build, or missing qualification
artifact is not parity evidence. The verdict records the exact run ID,
metadata hash, raw/summary/correctness/analysis hashes, build-manifest hashes,
and binary hashes it certifies. The policy verifier independently reopens the
summary, samples, correctness data, build manifests, both timed executables,
and CMake cache, then recomputes the statistical values instead of trusting
the run status.

The locked cross-backend rows cover the shared JSON surface, including an
Euler-filtered ideal. They do not exercise the Atom-native factored ingress or
replace the separate Euler phase-timing diagnostics; those paths require their
own Criterion/diagnostic evidence before receiving a performance claim.

The embedded standard MZV table does not inflate ordinary benchmark contexts:
the Atom API and JSON integration bridge reserve its small basis, plus only
non-basis constants transitively reachable from an expression that names
them. Explicit custom tables retain the wide compatibility context.

Runtime differential tests and timings require an available Symbolica license
instance on its configured singleton port. They must not be run concurrently
with another restricted Symbolica process, and results must never be inferred
from compile-only checks.
