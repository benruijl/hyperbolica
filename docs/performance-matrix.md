# Expanded exploratory performance matrix

The independent [matrix manifest](../tests/fixtures/performance-matrix.json)
resolves 191 workloads. It does not change the locked qualification corpus,
policy, or existing comparison helper. A successful exploratory run is not a
performance qualification, and a missing or failed comparison is never counted
as parity.

| Tier | Cases | Scope | Per-backend invocation bound |
|---|---:|---|---|
| `core` | 14 | Every checked-in benchmark workload | 60 s; 4 GiB address space |
| `api` | 79 | Every checked-in differential workload, including expected rejection paths | 20 s; 2 GiB address space |
| `scaling` | 65 | Deterministic degree, dimension, pole multiplicity, rational power/derivative, log weight, and period weight families | 60 s; 4 GiB address space |
| `attachment` | 4 | `tst0`, `tst1`, and both `findroots21` cases | 60–180 s; 4 GiB address space |
| `heavy` | 6 | `tst2`–`tst4`, parity face, and both quadruple boxes | 600–1,800 s; 8 GiB address space |
| `pipeline` | 23 | All remaining SubTropica graph inputs, explicitly listed as requiring preprocessing | Not launched |

The default selection is `core` plus `scaling`, or 79 workloads. Selecting a
subset still retains every inventory entry in the generated report as measured,
failed, requiring preprocessing, attachment missing, or not selected. Heavy
cases require explicit selection; a case-name glob overrides tier selection.

The 65 scaling cases comprise nine sparse multiplications (degrees 8, 32, 128
across 1, 3, 8 variables), six dense multiplications and six dense polynomial
powers (total degrees 4, 8, 12 across 2, 3 variables), six expression-parsing/rational-power cases
(exponents -3, -2, -1, 2, 3, 5), four rational derivatives with parameter-only
denominators (powers 1, 2, 4, 8), three multivariate GCDs (degrees 2, 4, 8), three
resultants (degrees 2, 4, 6), nine partial-fraction decompositions (2–4 symbolic
poles with multiplicities 1, 3, 6), three Laurent expansions (orders 8, 32, 128),
eight endpoint periods (weights 2, 4, 6, 8 on both supported intervals), four
logarithmic integrations (log weights 0–3), and four separable integrations
(1, 2, 3, 5 variables). The exact requests and their hashes appear in
`inventory.json`, so every scaling point is independently reproducible.
There is no separate rational-power or rational-derivative JSON operation:
rational powers use `parse_expr`, and derivatives use `differentiate_wordlist`
with an empty word. These supported workloads include parsing and dispatch;
they do not isolate the runtime of a particular native arithmetic wrapper.

## Running and inspecting the inventory

Python 3, Bash, jq, and Linux `prlimit` are required. Provide prebuilt executable
paths or Bash `exec` launchers. Supply the Symbolica license in the parent
environment; the runner inherits it without recording its value. The original
attachment is accepted only if its SHA-256 matches the manifest. No attachment
text is evaluated as code.

```bash
python3 scripts/performance_matrix.py --list \
  --attachment /absolute/path/to/hf_benchmarks.json

python3 scripts/performance_matrix.py \
  --reference /absolute/path/to/hyperflint-cpp \
  --candidate /absolute/path/to/hyperbolica-current \
  --parser /absolute/path/to/hyperbolica-current \
  --attachment /absolute/path/to/hf_benchmarks.json \
  --tier core --tier scaling --tier attachment \
  --pairs 5 --warmup 1 --cpu 36 \
  --output target/matrix-current-vs-cpp

python3 scripts/performance_matrix.py \
  --reference /absolute/path/to/hyperbolica-baseline \
  --candidate /absolute/path/to/hyperbolica-current \
  --parser /absolute/path/to/hyperbolica-current \
  --case 'scale.partial_fractions.*' \
  --pairs 5 --warmup 1 --cpu 36 \
  --output target/matrix-current-vs-baseline
```

Use an allowed logical CPU for the host. `--pairs` accepts any positive integer;
one pair is a probe, not a robust performance estimate. Evidence directories
must not already exist. `--artifact PATH` can be repeated to record hashes of
the underlying binaries, runtime libraries, and build manifests when measured
paths are launchers. The runner hashes the measured executables, parser, matrix,
driver and comparison scripts, standard MZV table, source fixtures, resolved
requests, and supplied artifacts. It does not claim to
verify build flags or bind a launcher automatically to its underlying binary.

## What is checked and measured

Each selected case first runs once in both backends. Successful results must
compare exactly under their existing fixture policy: byte equality, declared
telemetry normalization, or exact Symbolica canonicalization of declared
algebraic fields. All undeclared response fields and shuffle keys remain exact.
The matrix reuses the audited response-comparison shell implementation.

Algebraic-letter metadata is checked separately even when the transport field
is ignored by a fixture. The adapter compares root indices, variable identity,
defining polynomial, discriminant, sum, product, derived leading coefficient,
and any additional fields. It validates reported root symbols, a supplied
leading coefficient, and the discriminant/Vieta relation. Thus different C++
and Rust field names do not cause this mathematical metadata to disappear.

Only a successful preflight permits warmups and adjacent alternating pairs.
Every timed response is checked against that backend's preflight response;
failure or nondeterminism stops sampling that case and leaves its ratio blank.
The raw algebraic-letter table and variable-index mapping must also match that
backend's preflight exactly, even when the cross-backend fixture ignores their
transport fields. This prevents changing root assignments from passing a
timing repeatability check.
Both backends are attempted in the preflight even if the first rejects or
exceeds its bounds. Later cases still run. Expected rejection fixtures are
reported as backend rejections, not successful timed calculations.
An expected rejection is not itself a performance regression or an
implementation bug; it simply cannot provide a successful-calculation speed
ratio. The original differential harness remains the gate for those contracts.

Times include fresh process/launcher startup, JSON parsing, computation,
serialization, and teardown. They exclude comparison, warmup, and compilation.
Monotonic wall time and `wait4` CPU time/peak RSS come from the existing process
measurement helper, started fresh for every invocation. Only the helper's
inner backend metrics are recorded; helper startup and helper resource use are
excluded. This prevents a growing matrix parent's inherited pre-exec memory
from inflating backend peak RSS. The driver and descendants inherit the selected CPU;
algebra thread settings are fixed at one. Algorithm-selection environment
overrides are removed. The report contains medians, geometric means of paired
candidate/reference ratios, observed ratio range, and peak RSS. It deliberately
does not infer confidence intervals or a universal parity claim.

Timeouts terminate the entire process group. Linux `RLIMIT_AS` constrains
address space in the backend and descendants, and `RLIMIT_FSIZE` limits each
output file to 64 MiB; core dumps are disabled. Address space is not RSS and is
not an aggregate limit across a process tree. These backends normally perform
their algebra in one process. Allocation failures are labeled as allocation or
limit failures, since an abort does not prove the operating system's global
memory limit was reached. Raw output, stderr, exit status, elapsed time, and RSS
remain available for every attempted invocation, including failures.

`results.json`, `summary.csv`, and `overview.md` are updated after each case;
`inventory.json` contains all resolved workloads from the beginning. Exit code
1 means at least one selected case failed validation/execution/sampling, while
the remaining cases were still processed. Exit code 2 indicates a configuration
or infrastructure error. Preprocessing and unselected rows do not count as
successful comparisons.

## Every attachment input is accounted for

The attachment contains 33 cases, with SHA-256
`909be0ef1bc030a00ea823190733329c228a183e8f8e55ff909c56000c461e3c`.
The ten direct cases retain their exact integrand and integration order. The
two `findroots21` requests explicitly enable algebraic letters. The manifest
sets `check_divergences=false` for attachment reproduction, matching the
previous attachment benchmark, and records this in each resolved request.

| Attachment index (zero-based) | Direct backend case | Tier | Seconds / MiB address-space cap |
|---:|---|---|---:|
| 0 | `tst0` | attachment | 60 / 4096 |
| 1 | `tst1` | attachment | 180 / 4096 |
| 2 | `tst2` | heavy | 900 / 8192 |
| 3 | `tst3` | heavy | 1800 / 8192 |
| 4 | `tst4` | heavy | 1800 / 8192 |
| 5 | `findroots21_a` | attachment | 60 / 4096 |
| 6 | `findroots21_b` | attachment | 60 / 4096 |
| 7 | 3-loop parity face | heavy | 600 / 8192 |
| 8 | one-mass quadruple box | heavy | 600 / 8192 |
| 9 | collaborator-convention quadruple box | heavy | 600 / 8192 |

The remaining 23 entries are individually retained as `pipeline` rows:

| Index | Input |
|---:|---|
| 10 | `dbox1m` |
| 11 | one-mass nonplanar 3-loop form factor |
| 12 | `tri_1L`, literal zero |
| 13 | `bub_1L` |
| 14 | `sunset_2L` |
| 15 | `ss2L_mid` |
| 16 | `p3L_a` |
| 17 | `vac3L` |
| 18 | `vac4L` |
| 19 | `vac5L` |
| 20 | `box_1L` |
| 21 | `pent1L` |
| 22 | lib10 equal-mass bubble |
| 23 | lib10 triangle with one massive propagator |
| 24 | lib10 box with two massive propagators |
| 25 | lib10 equal-mass theta |
| 26 | lib10 massless parachute |
| 27 | lib10 bubble-pentagon with one massive propagator |
| 28 | lib10 one-mass three-loop Sudakov form factor |
| 29 | triangle ladder L=1 |
| 30 | triangle ladder L=2 |
| 31 | triangle ladder L=3 |
| 32 | triangle ladder L=4 |

Twenty-two have epsilon-dependent powers and projective integration measures.
They require the same SubTropica gauge selection, analytic-continuation and
subtraction terms, and requested epsilon order before either integration
backend can be compared. Symbolica can perform local epsilon expansions after
that preparation. Expanding the raw unregularized integrand is not a substitute
for it. The literal-zero triangle also needs a defined frontend handling of
unused integration variables. No frontend output bundles or epsilon order were
supplied, so these entries are not converted into misleading direct requests.

## Relation to previous reports

The [2026-09-07 audit](performance-audit-20260907.md) reports 18 selected fresh
timings, not coverage of this larger matrix. Its conclusion that `tst1` remains
slower applies to its recorded binaries and samples. The earlier
`target/attachment-benchmark/OVERVIEW.md` describes a historical implementation
before the period-tuple integration work; its statement that production tuples
were missing is no longer a description of current source.

Neither report freshly measured `tst3` or `tst4`. Earlier single-sample probes
completed `tst2`, timed out Rust on parity face, and hit imposed bounds on both
quadruple boxes. Those historical outcomes explain explicit heavy tiers and
resource limits here; they are not carried forward as measurements of a new
binary. Run the selected rows again to obtain current evidence.

Harness-only verification:

```bash
python3 -m unittest discover -s tests -p test_performance_matrix.py -v
```
