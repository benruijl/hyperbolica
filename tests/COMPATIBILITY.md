# Compatibility and performance harness

`json_cli_compat.rs` checks representative algebra, word, conversion,
linear-reducibility, reduction, primitive, integration-step, and full-driver
requests through the shipped CLI. The cases execute serially because an
unlicensed Symbolica instance is bound to one calling thread.

`scripts/differential.sh` treats the C++ HyperFLINT executable strictly as an
external oracle. Set `HYPERFLINT_CPP` and `HYPERFLINT_RUST` to select the two
executables. It never links FLINT or C++ code into the Rust crate. Stable
responses in `fixtures/differential.jsonl` are compared byte for byte.

`scripts/lint-fixtures.sh` is the CAS-independent preflight for this corpus and
the benchmark corpus. It validates the checked-in schemas, canonical one-record
JSONL form, unique names and requests, and the narrow metadata allowlist below.
Both runtime harnesses invoke it before building or executing a backend.

Four response classes cannot be byte-compared:

- LR responses contain a backend version, a floating score, wall time, and may
  choose either order when scores tie exactly. Both orders are validated as
  permutations before that field is removed.
- Specific-order LR responses remove only the backend version and wall time.
  The verdict, malformed flag, blocking step/degree/letter, forbidden-variable
  diagnostic, carry profile, strategy, and inert search envelope remain exact.
- MZV reduction exposes different-but-compatible context variable sets.
- Integration-step responses expose the backend-owned augmented MZV context.
- Full integration adds wall time and may explicitly emit a null
  algebraic-letter field.

Those fields are named in each fixture and removed before a canonical JSON
comparison. Mathematical result fields remain exact.

`scripts/benchmark-compare.sh` builds the Rust release binary, warms both CLI
binaries, verifies exact output equality, and reports median end-to-end times.
The default gate fails when Rust is both more than 1.20 times slower and more
than 2 ms slower. Useful knobs are `WARMUP`, `ITERATIONS`, `THREADS`,
`MAX_SLOWDOWN`, `REGRESSION_TOLERANCE_MS`, `CPUSET`, `RESULTS_FILE`, and
`FAIL_ON_REGRESSION`. The benchmark includes process startup by design; no
persistent protocol exists in either compatibility CLI.

Every benchmark invocation records its unchanged five-column summary CSV,
per-iteration raw samples, and reproducibility metadata. See
`docs/verification.md` for the evidence layout and interpretation rules.
