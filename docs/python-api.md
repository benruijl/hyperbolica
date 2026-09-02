# Python API

Hyperbolica's standalone wheel embeds the vendored Symbolica Python API and
the Hyperbolica algorithms in one native extension. Mathematical input and
output use that extension's native `Expression` class throughout; the binding
does not format and reparse expressions.

## One expression identity

Import Symbolica constructors and Hyperbolica operations from the same module:

```python
import hyperbolica as hb

x = hb.S("x")
integrand = 1 / (x + 1) ** 2
assert type(integrand) is hb.Expression
answer = hb.integrate(integrand, [x])
assert type(answer) is hb.Expression
```

Do not construct input with a separately installed `symbolica` wheel. PyO3
class identity belongs to the compiled extension, so an `Expression` from a
different wheel is intentionally rejected instead of being serialized. The
standalone `hyperbolica` package exposes Symbolica's broader runtime API as
well as the stable subset described by its bundled typing stub.

`copy.copy`, `copy.deepcopy`, and pickle round trips preserve this exact
embedded `Expression` class. The standalone registration installs an
exact-type `copyreg` reducer that uses Symbolica's portable Atom export/import
format and captures its package-local reconstructor; it does not import or
alias a separate `symbolica` package. The installed test harness verifies
same-process copies, an import-on-unpickle fresh process, and a fresh process
whose symbol-registration order was deliberately perturbed.

## Direct and prepared integration

`integrate(expression, variables, options=None)` returns one collected native
expression. `integrate_detailed(...)` returns an immutable
`IntegrationResult` with the expression, variables, exact indeterminates,
formal algebraic-letter metadata, and term counts.

Use `prepare(...)` when integrating the same lowered input more than once:

```python
options = hb.IntegrationOptions(parallel=False)
prepared = hb.prepare(integrand, [x], options)

# Preparation owns a copy. Later mutation does not change its defaults.
options.parallel = True
assert prepared.options.parallel is False

answer = prepared.integrate()
detailed = prepared.integrate_detailed(
    hb.IntegrationOptions(check_divergences=True)
)
```

`PreparedIntegral`, `IntegrationResult`, and `AlgebraicLetter` are immutable.
Their `copy.copy` and `copy.deepcopy` operations return equivalent independent
Python handles backed by shared immutable Rust storage. `IntegrationOptions`
is mutable and compares by value. Its default MZV table is embedded in the
extension and stored behind cheap shared Rust storage; no data directory or
separate CAS package is needed at runtime.

Omitting both `mzv_reductions` and `mzv_basis` selects that standard table.
Passing either argument explicitly selects a complete override, so an empty
table is intentional and remains possible:

```python
standard = hb.IntegrationOptions()
without_mzv_reductions = hb.IntegrationOptions(mzv_reductions=[])
```

The standard table is eagerly expanded into its small basis when a generated
period constant is needed. Preparation reserves only those basis atoms rather
than adding every generated reduction left-hand side to every polynomial
context.

## Directed intervals

`integrate_over(expression, variables, intervals, options=None)` and
`integrate_detailed_over(...)` accept exactly one `(from, to)` pair per
integration variable. Endpoints are native `Expression` objects. Use
`Symbol.INFINITY` and `-Symbol.INFINITY` for directed real infinity; complex
infinity is rejected before integration.

```python
x, a = hb.S("x", "a")
options = hb.IntegrationOptions(check_divergences=True, parallel=False)

finite = hb.integrate_over(hb.N(1), [x], [(hb.N(2), hb.N(5))], options)
assert finite == hb.N(3)

tail = hb.integrate_detailed_over(
    1 / (x + 1) ** 2,
    [x],
    [(a, hb.Symbol.INFINITY)],
    options,
)
assert tail.expression == 1 / (a + 1)
```

Intervals are directed, so reversing endpoints reverses the sign. Finite
bound parameters are preserved as exact spectator indeterminates even when
they do not occur in the integrand. Prepared values made by `prepare(...)`
cover the default `[0,+Infinity)` domain; interval-aware reuse is currently
available through the Rust `prepare_atom_over` API. The reverse all-infinite
domain `(+Infinity,-Infinity)` remains unsupported by the upstream interval
rescaler; use the supported `(-Infinity,+Infinity)` direction and negate the
result when needed.

## Errors

Every Hyperbolica exception derives from `HyperbolicaError`:

| Exception | Meaning | Structured attributes |
| --- | --- | --- |
| `InputError` | Invalid expression, schedule, or options | — |
| `DuplicateVariableError` | Repeated integration variable | `variable` |
| `AlgebraError` | Exact algebra failure | — |
| `ContextError` | Symbol outside an exact context | `variable` |
| `DivergentIntegralError` | Non-cancelling endpoint divergence | `boundary`, `variable`, `log_power`, `power` |
| `UnsupportedFeatureError` | Reserved for unsupported public features | — |

The human-readable explanation remains in `str(error)` and `error.args[0]`;
callers should use the subclass and attributes for program logic.

## Introspection and versioning

The installed package exports a deterministic `__all__` plus:

- `__version__`: Hyperbolica package version;
- `__symbolica_version__`: version of its embedded Symbolica kernel;
- `__api_version__`: integer Python API compatibility level;
- `__license__`: mixed-distribution notice (MIT for Hyperbolica code, separate
  terms for bundled Symbolica);
- `__symbolica_license__`: explicit Symbolica redistribution warning.

Public functions and methods provide Python text signatures and docstrings, so
`help()` and `inspect.signature()` work without consulting the Rust sources.

## Typing and packaging checks

The repository-root `hyperbolica.pyi` is Maturin's supported pure-Rust layout.
Maturin installs it as `hyperbolica/__init__.pyi` and adds `py.typed`; it does
not require a second Python package directory. The stub declares the stable
Hyperbolica surface and the small embedded-Symbolica surface needed to build
native integration inputs. It deliberately does not alias a separate
`symbolica.Expression` type.

Run the static and wheel-layout checks with:

```sh
scripts/check-python-static.sh
scripts/check-python-package.sh
```

Both scripts accept explicit `PYTHON`, and the first also accepts `PYRIGHT`
while the second accepts `MATURIN`.

The `python-extension` feature fixes PyO3's stable-ABI floor at CPython 3.10.
The package gate builds in release mode and rejects a wheel unless both its
filename and WHEEL metadata carry `cp310-abi3`, even when the selected build
interpreter is newer.

After `maturin develop --release` or installing a wheel, run:

```sh
scripts/test-python-installed.sh
```

The ordinary installed-extension suite validates identity, signatures,
copies, cross-process pickle reconstruction, exports, and error paths without
performing integration. Licensed integration cases are skipped unless
explicitly enabled:

```sh
HYPERBOLICA_RUN_LICENSED_TESTS=1 scripts/test-python-installed.sh
```

Licensed tests hold a POSIX advisory lock for the whole suite. Set
`HYPERBOLICA_LICENSE_LOCK` to a shared path when multiple local or CI jobs use
the same Symbolica license endpoint. Run the suite serially on platforms that
do not provide `fcntl`.
