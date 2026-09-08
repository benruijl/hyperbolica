# Compatibility and performance harness

`json_cli_compat.rs` checks representative algebra, word, conversion,
linear-reducibility, reduction, primitive, integration-step, and full-driver
requests through the shipped CLI. The cases execute serially because an
unlicensed Symbolica instance is bound to one calling thread.

`scripts/differential.sh` treats the C++ HyperFLINT executable strictly as an
external oracle. Set `HYPERFLINT_CPP` and `HYPERFLINT_RUST` to select the two
executables. It never links FLINT or C++ code into the Rust crate. Stable
responses in `fixtures/differential.jsonl` are compared byte for byte.

## Per-call thread budget

For the full-driver `hyperflint` operation, the compatibility bridge honors
`HF_MAX_THREADS_PER_CALL` at request entry. The pinned C++ implementation at
SubTropica `adfd3af` reads the variable only in `hyperflint_sym` and applies a
positive `atoi` result; therefore unset, empty, non-numeric, zero, and negative
values mean no override, while a value such as `"  +2workers"` selects two.
Values outside the positive C `int` range are ignored because upstream `atoi`
does not define them.

Rust does not mutate Rayon's process-global pool. A limit greater than one is
implemented with a request-scoped pool, bounded further by the process CPU
allocation and Symbolica license. A limit of one uses the caller thread and
disables the bridge's coarse independent-entry fanout. This preserves the
memory-control intent and permits concurrent C ABI/LibraryLink requests without
a shared thread-count race. The limit is a ceiling; `"parallel": false` can
still request stricter serial execution. Other JSON operations intentionally
ignore this variable, matching the pinned handler scope.

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

Fixtures whose backends may print the same expression with different term
ordering use `compare: "semantic"`. Only the explicitly declared
`semantic_fields` are reparsed by the Rust executable and replaced with their
Symbolica canonical Atom form; the surrounding response is still compared
exactly after the same narrow metadata ignores. The C++ oracle runs with all
Symbolica license variables removed, while the Rust backend and semantic
parser may inherit the caller's license.

Full-integration fixtures use the explicit `result[].coef` semantic field to
canonicalize every coefficient while retaining the shuffle keys and response
envelope for exact comparison.

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
