# HEPkit definite integration

The Python API is part of the Symbolica community wheel:

```python
from symbolica import E, S, Expression, Symbol
from symbolica.community.hepkit import integration

x, a = S("x", "a")
f = 1/(x+1)**2
options = integration.IntegrationOptions(check_divergences=True, parallel=False)
answer = integration.integrate(f, [x], options)
assert type(answer) is Expression
assert answer == E("1")
tail = integration.integrate_over(f, [x], [(a, Symbol.INFINITY)], options)
assert (tail - 1/(a+1)).cancel() == E("0")
```

## Contract

`integrate(expression, variables, options=None)` integrates over `[0,+Infinity)`
in variable order. `integrate_over(expression, variables, intervals, options=None)`
uses one directed `(from,to)` pair per variable. Reversing an interval changes
its sign; finite endpoint parameters are retained as exact indeterminates.
Complex infinity and the reversed whole-real interval remain unsupported.

`integrate_detailed` and `integrate_detailed_over` return an immutable
`IntegrationResult`, including the expression, integration variables,
indeterminates, formal algebraic letters and collected term counts.
`prepare(expression, variables, options=None)` lowers a default-domain input
once. `PreparedIntegral.integrate()` and `.integrate_detailed()` reuse it.
Prepared interval inputs are not exposed in this Python version.

Constant periods are evaluated using the configured MZV table. Remaining
regularized periods at infinity are converted to unit-interval words before
being returned as finite-endpoint `Hlog` expressions; changing just the
endpoint would change their mathematical value.

`integration.mzv_symbol()` returns the registered multiple-zeta-value function
head as a native Symbolica expression. For example, `MZV = integration.mzv_symbol()`
and `MZV(3)` construct the same exact constant used in integration results,
without depending on its internal namespace. This formal head does not evaluate
numerically. For a depth-one reference, use Symbolica's built-in Riemann zeta:
`(6 * E("3").zeta()).evaluate({}, decimal_digit_precision=40).real`.

Preparation captures an independent copy of mutable `IntegrationOptions`.
Prepared inputs, results and algebraic-letter metadata are immutable and
support shallow/deep copying through shared immutable Rust storage. Expressions
use Symbolica's existing copy/pickle implementation; integration installs no
alternative expression type or serializer. In the selected Symbolica kernel,
pickle stores process-local symbol IDs: use `expression.save(filename)` and
`Expression.load(filename)` for persistence across interpreters or differing
symbol-registration orders. Fresh-process save/load is tested explicitly.

Options retain `check_divergences=False`, `parallel=True`,
`introduce_algebraic_letters=False`, `close_final_positive_letters=True`, and
the embedded standard MZV table. Supplying either `mzv_reductions` or `mzv_basis`
selects an explicit replacement; empty input intentionally disables reductions.
`check_divergences=True` requests detection of uncancelled endpoint divergences.

Native parallelism respects Symbolica's license limits. Browser execution is
serial regardless of the parallel option, without initializing a Rayon pool.
The MZV table is embedded and requires no runtime filesystem setup.

## Errors and identities

All errors derive from `IntegrationError` in
`symbolica.community.hepkit.integration`:

| Exception | Structured fields |
|---|---|
| `InputError` | none |
| `DuplicateVariableError` | `variable` |
| `AlgebraError` | none |
| `ContextError` | `variable` |
| `DivergentIntegralError` | `boundary`, `variable`, `log_power`, `power` |
| `UnsupportedFeatureError` | none |

The package exports version metadata (`__version__`, `__symbolica_version__`,
`__api_version__`) and a deterministic `__all__`. Hyperbolica usage contributes
citations for Hyperbolica, HyperInt and SubTropica to the host's
`symbolica.get_citations()`; importing alone does not. These credit the Rust
implementation, Panzer's hyperlogarithmic integration algorithms, and the
upstream HyperFLINT implementation and data, respectively. Each entry includes
a short project description, reasons for its inclusion, and BibTeX metadata.

## Existing HEPkit functionality

Use `from symbolica.community.hepkit import ibp` for HEPkit's existing family,
rule and solution classes. IBP retains its native-only availability; the
integration package evaluates integrals and does not re-export IBP.
This release does not infer parameter
integrals, normalization, analytic continuation, or epsilon expansion, and does
not automatically evaluate master integrals after reduction.

HEPkit remains responsible for kinematics, integral families, Symanzik
polynomials and reduction. The community example `examples/hep_integration.py`
uses its existing Symanzik API for a convergent D=2 bubble, explicitly choosing
unit masses, p²=0, powers (1,1), projective gauge x2=1, and stripping the
loop-measure prefactor. The parameter integral is exactly one.

`Expression.integrate(x)` keeps its existing antiderivative semantics from
Symbolica-integrate. That crate and its dispatch are unchanged.

## Building and migrating

Replace `import hyperbolica as hb` with
`from symbolica.community.hepkit import integration as hb`; import `S`, `E`,
`Expression`, and `Symbol` from `symbolica`. Replace `HyperbolicaError` with
`IntegrationError`. No standalone Hyperbolica wheel or compatibility alias is
provided.

Hyperbolica exposes `python::CommunityModule` through Cargo's `python` feature.
The host disables Hyperbolica's default features and selects `native` or `wasm`.
Only the top-level host enables PyO3 extension/ABI features. Root Cargo patches
select the development Symbolica source; consumers select their shared source.
Native standalone Rust defaults retain GMP/MPFR and the optional faster allocator;
the community wheel keeps its existing system allocator.

The `python_stubgen` feature supplies metadata to the community stub generator.
Only exception declarations and version metadata require a handwritten typing
supplement; expression types come from Symbolica's canonical declarations.

Build and install the community wheel using its normal Maturin/Pyodide workflow.
Run `scripts/test-python-installed.sh` and `scripts/check-python-static.sh`
against that environment (`PYTHON` and `SYMBOLICA_COMMUNITY_DIR` are configurable).
The community suite uses the same exact fixtures in native pytest and its
Pyodide installed-wheel harness.
