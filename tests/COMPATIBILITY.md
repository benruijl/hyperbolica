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

Five response classes cannot be byte-compared:

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

`scripts/benchmark-compare.sh` builds the LTO Rust release binary, validates
fixture-controlled response equality, and measures adjacent balanced backend
pairs with monotonic wall time, process CPU time, and peak RSS. Process startup
is included by design; neither compatibility CLI has a persistent protocol.
The default exploratory mode accepts `PAIRS` (or legacy `ITERATIONS`),
`WARMUP`, `THREADS`, `CPUSET`, and the three explicit ratio thresholds.
It always records `qualified: false`.

Set `BENCHMARK_MODE=qualification` for the locked on-par-or-better gate.
That mode rejects altered sampling, affinity, thread, bootstrap, threshold, or
corpus settings and requires the optimized pinned C++ baseline. Every run
fresh-builds both timed backends, records the five-column summary, paired raw
samples, correctness hashes, statistical analysis, build manifests, and a
self-bound policy verdict. The supplied C++ path only locates its verified
source checkout for the fresh CMake build. See
`docs/verification.md` for the artifact layout and strict claim rules.
