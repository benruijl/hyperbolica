# Hyperbolica

Hyperbolica is a Rust port of
[SubTropica/HyperFLINT](https://github.com/SubTropica/SubTropica/tree/main/HyperFLINT)
for exact hyperlogarithmic integration and linear-reducibility analysis. Its
production API accepts Symbolica `Atom` values directly, and its only computer
algebra backend is the official Symbolica `dev` checkout in
`vendor/symbolica`.

The port is under active development. The Rust core, typed Atom API, JSON
compatibility adapter, C ABI, and optional PyO3 layer are present. Differential
fixtures and benchmark gates are checked against an external C++ HyperFLINT
executable; the C++ program is an oracle only and is never linked into or
invoked by the library.

## Pure-Symbolica contract

- Production code has no FLINT, Singular, msolve, or other CAS dependency.
- Polynomial, rational-function, resultant, factorization, and Gröbner-basis
  work stays inside Symbolica.
- Every special head owned by Hyperbolica is declared by one `initialize!`
  block in `src/symbols.rs`. Indexed objects such as MZVs and algebraic
  letters are function atoms of those registered heads, not dynamically named
  symbols.
- String and JSON parsing exists only at compatibility boundaries. New Rust and
  Python callers should pass native Symbolica expressions.

Run the dependency and source audit with:

```sh
scripts/check-pure-symbolica.sh
```

The exact upstream base, local patch series, and checkout command are recorded in
[`vendor/SYMBOLICA_SNAPSHOT.md`](vendor/SYMBOLICA_SNAPSHOT.md). The former
`vendor/symbolica-src` source copy is retained only as a historical audit
artifact and is not selected by Cargo.

The historical symbolized diagnostic profile, including resultant-backend and
partial-fraction hotspots, is in
[`docs/performance-profile.md`](docs/performance-profile.md).

## Build and test

Rust 1.89 or newer is required.

```sh
git clone --branch dev --single-branch \
  https://github.com/symbolica-dev/symbolica.git vendor/symbolica
git -C vendor/symbolica checkout fb845d34bda8ccf1fedef6544d3aa46dc24944e3
git -C vendor/symbolica switch -c codex/hyperlica-dev-20260908
git -C vendor/symbolica am ../symbolica-hyperlica-dev-20260908.patch
git -C vendor/symbolica am ../symbolica-heap-scratch-20260908.patch
scripts/check-pure-symbolica.sh
cargo build --release
cargo test --locked --lib --tests --bins --examples
cargo test --locked --benches
cargo clippy --locked --all-targets --all-features -- -D warnings
scripts/check-examples.sh
```

Symbolica's unlicensed mode permits one process and one core. On an unlicensed
machine, use the serialized harness instead of Cargo's parallel test runner:

```sh
scripts/test-unlicensed.sh
```

The harness actively removes both Symbolica credential variables, runs every
test in its own single-threaded process, and forces bridge requests onto the
serial path. Direct integration consults Symbolica's licensed thread limit
before entering its Rayon branch.

If another Symbolica process already owns the machine-wide community-license
port, compilation and `--no-run` checks still work, but executing Symbolica
code must wait for that process to release the port.

## Atom-native Rust API

Callers introduce their ordinary symbols with Symbolica and pass the
integration variables explicitly. Hyperbolica discovers any additional
function indeterminates structurally.

```rust
use hyperbolica::prelude::*;

fn example() -> AtomIntegrationResult<Atom> {
    let (x, y) = symbol!("example_x", "example_y");
    let integrand = Atom::one() / ((x + 1).pow(2) * (y + 1).pow(2));

    let prepared = prepare_atom(&integrand, &[x, y])?;
    let result = integrate_prepared_atom(
        &prepared,
        &AtomIntegrationOptions {
            check_divergences: true,
            parallel: false,
            ..AtomIntegrationOptions::default()
        },
    )?;

    result.to_atom()
}
```

`PreparedAtomInput` is reusable when the same expression is integrated with
different options. `AtomIntegrationOutput` retains structured terms and can be
materialized as one normalized `Atom`. See
[`examples/atom_integration.rs`](examples/atom_integration.rs) and
[`examples/atom_hlog.rs`](examples/atom_hlog.rs).

Directed finite, semi-infinite, and whole-real-line domains use native
Symbolica endpoints rather than string sentinels. Use `integrate_atom_over`,
or prepare a reusable value with `prepare_atom_over` and call
`integrate_prepared_atom_over`. Parameters that appear only in finite bounds
remain exact spectator variables. See
[`examples/atom_intervals.rs`](examples/atom_intervals.rs).

## Python API

The standalone extension embeds the vendored Symbolica Python API and
Hyperbolica in one shared kernel. Construct expressions with the `S`, `E`, and
`Expression` objects shipped at the `hyperbolica` top level; those are the
exact PyO3 types accepted by `prepare` and `integrate`, with no formatting or
reparsing across the boundary.

```sh
python -m pip install maturin
maturin develop --release
scripts/test-python-installed.sh
```

```python
import hyperbolica as hb

x = hb.S("x")
integrand = 1 / (x + 1) ** 2
options = hb.IntegrationOptions(parallel=False)
prepared = hb.prepare(integrand, [x], options)
result = prepared.integrate(options)
```

For a non-default domain, supply one directed endpoint pair per variable:

```python
a = hb.S("a")
result = hb.integrate_over(
    1 / (x + 1) ** 2,
    [x],
    [(a, hb.Symbol.INFINITY)],
    hb.IntegrationOptions(check_divergences=True, parallel=False),
)
```

The root `pyproject.toml` selects the `python-extension` feature for maturin
wheel builds. The lower-level `python` feature supports Rust-side binding
tests without PyO3's extension-module linker mode. The supported packaged API
is currently the standalone wheel. Its standard MZV reductions are embedded,
so installed integration does not need a source-tree data directory or a
separate CAS package. A future Symbolica community package needs
separate wrappers whose declared module paths follow
`symbolica.community.hyperbolica`; the top-level standalone classes are not
silently reused for that incompatible layout. Do not pass objects from a
separately compiled `symbolica` wheel into the standalone `hyperbolica`
extension, because PyO3 class identity is specific to the compiled module.

The wheel is PEP 561 typed. See [`docs/python-api.md`](docs/python-api.md) for
the complete object model, exception data, packaging checks, and the explicitly
opt-in licensed test suite.

## Compatibility CLI

The binary retains the upstream name for migration and differential testing:

```sh
cargo run --release --bin hyperflint -- factor 'x^4-y^4'

printf '%s\n' \
  '{"op":"resultant","a":"x^2+y*x+1","b":"x-y","var":"x","vars":["x","y"]}' \
  | cargo run --release --bin hyperflint -- eval-json
```

The upstream memory-control setting is also supported for full integration:

```sh
HF_MAX_THREADS_PER_CALL=1 cargo run --release --bin hyperflint -- eval-json \
  <<<'{"op":"hyperflint","vars":["x"],"vars_int":["x"],"f":"1/(1+x)^2"}'
```

It caps only `hyperflint` requests and is request-scoped, including through the
C ABI and LibraryLink adapter. Exact parsing and concurrency behavior are
documented in [`tests/COMPATIBILITY.md`](tests/COMPATIBILITY.md).

The accepted JSON operations and oracle workflow are documented in
[`tests/COMPATIBILITY.md`](tests/COMPATIBILITY.md). This adapter deliberately
does not define the production Rust API.

## Wolfram LibraryLink

An isolated `librarylink/` crate implements the nine-symbol surface loaded by
the pinned SubTropica release. It is a separate dynamic library because the
upstream LibraryLink callbacks and stable C ABI reuse several names with
incompatible signatures. Build one local development stage containing the CLI,
stable C-ABI backend, LibraryLink adapter, headers, and license notices with
`scripts/stage-local.sh`. The adapter defaults to the pinned SubTropica
compatibility version `1.2.13`; the stage metadata records that value and marks
the bundle non-redistributable. Verify exports, C/C++ ABI layout, lifecycle,
UTF-8 marshalling, and—with a Symbolica license set—the six upstream LR
strategy rows with `scripts/check-librarylink.sh`. See
[`docs/librarylink.md`](docs/librarylink.md) for version stamping and a
`LibraryFunctionLoad` example.

## Performance verification

Microbenchmarks cover hot exact-algebra kernels:

```sh
cargo bench --bench core_algebra
cargo bench --bench lr_structural_keys
cargo bench --bench semantic_keys
cargo bench --bench euler
```

End-to-end comparisons consume identical JSONL fixtures and compare the Rust
binary with a separately built C++ HyperFLINT executable:

```sh
HYPERFLINT_CPP=/absolute/path/to/hyperflint scripts/differential.sh
HYPERFLINT_CPP=/absolute/path/to/hyperflint scripts/benchmark-compare.sh

# The publishable gate is deliberately opt-in.
BENCHMARK_MODE=qualification \
  HYPERFLINT_CPP=/absolute/path/to/release-portable/hyperflint \
  scripts/benchmark-compare.sh
```

The locked qualification gate uses 12 adjacent, balanced backend pairs for
each workload, a stratified bootstrap upper confidence bound, a severe
per-workload limit, and a peak-RSS limit. The ordinary RSS limit remains
1.25; the dense resultant alone has a documented 1.60 override recording the
accepted exact-division workspace tradeoff. It requires the complete checked-in
corpus, sanitized one-thread child environments, fixed CPU affinity, clean
root and nested Symbolica source revisions, the LTO Rust profile, and
upstream's optimized `release-portable` C++ profile. Qualification first runs
the pure-Symbolica gate and binds the nested checkout provenance into the Rust
build manifest. After building Rust in a fresh isolated target directory, it
reruns that audit and rejects any revision, branch, remote, or worktree-root
change. It then clean-builds the pinned C++ source in a fresh CMake tree; the
supplied C++ path is used only to locate that source checkout. The default mode
is exploratory and can never claim qualification. No parity claim may be made
from compilation, an exploratory run, or a failed/missing qualification
artifact; see
[`docs/verification.md`](docs/verification.md).

## Design

- `api` — typed Atom preparation, options, integration, and structured output
- `symbols` — the central registered-head set plus typed words, Hlogs, and MPLs
- `core` — Symbolica-backed polynomial/rational data and exact coefficients
- `algebra` — shuffle algebra, factors, partial fractions, and Euler tests
- `integrator` — transformation, primitives, regularization, and LR search
- `series` — exact Hlog/MPL/Laurent expansions
- `reduce` — periods, contours, MZVs, and fibration
- `python`, `c_abi`, and the separate `librarylink/` crate — thin
  foreign-language boundaries over the typed core
- `bridge` — legacy string/JSON transport isolated from production APIs

The detailed dependency and representation rules are in
[`docs/architecture.md`](docs/architecture.md). The complete Symbolica API
review and retain/wrap/replace decisions are in
[`docs/symbolica-api-audit.md`](docs/symbolica-api-audit.md).

## License

Hyperbolica-authored code is MIT licensed. The selected Symbolica checkout
provides the separate Symbolica Source-Available License 1.0; the archived
source copy retains its own notice. Runtime rights are not included or
transferred by Hyperbolica. This checkout contains local Symbolica modifications;
check the applicable terms and permissions before publishing it or built artifacts. See
[`DISTRIBUTION-LICENSE.md`](DISTRIBUTION-LICENSE.md) and
`vendor/symbolica/License.md` in the selected checkout.
