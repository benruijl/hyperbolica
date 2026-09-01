# Symbolica public-API audit and migration map

Status: source audit of the vendored Symbolica snapshot used by this repository, 2026-08-31.

This document is a migration decision record, not a claim that an API name alone makes a replacement correct or faster. Every positive capability below was checked in the public Rust source, including the implementation bounds and relevant tests. Performance statements marked *expected* are hypotheses that must pass the benchmark gates near the end of this document.

## Scope and verdict vocabulary

The selected dependency is Symbolica 2.2.0 at `vendor/symbolica-src` (`vendor/symbolica-src/Cargo.toml:5-17`), not the other vendor snapshots that may be present in the workspace. Source anchors below are relative to `vendor/symbolica-src/src` unless a different root is stated.

- **REPLACE**: delete the hand-written CAS algorithm and call the named public Symbolica operation.
- **WRAP**: preserve Hyperbolica's type, validation, error, or output contract around a native Symbolica representation/operation.
- **RETAIN**: no equivalent native capability was found, or the specialized representation is plausibly important enough to retain pending measurement.
- **ORACLE**: use the native operation for differential testing before deciding whether it belongs on the hot path.

The audit covered `src/**`, Symbolica's Rust source and in-tree tests, and public trait bounds. A negative text search is evidence only for this exact 2.2.0 snapshot; it is not a claim about every Symbolica release.

## Executive migration order

1. **P0 — enforce the Atom/symbol contract and fix correctness.** Keep public input as a concrete `Atom`; use `AtomView` internally. Register every library-owned head once in `src/symbols.rs`. Represent indexed objects as function indeterminates such as `Wm(1)`, `MZV(3,5)`, and `Period(7)`, never as dynamically minted string symbols. Change `heads()` to lookup the initialized symbols with `get_symbol!` instead of rebuilding through `symbol!` on every call. Fix the missing conventional discriminant sign described below.
2. **P1 — make native rational polynomials the core.** Replace `Rat { Poly<Q>, Poly<Q> }` with a checked newtype around `RationalPolynomial<IntegerRing, u16>`. Preserve `PolyCtx`, errors, signed powers, and Atom conversion at the wrapper. This removes duplicated normalization, denominator-GCD addition, cross-cancelled multiplication, quotient-rule differentiation, and partial-fraction conversion round trips.
3. **P1 — finish the direct native partial-fraction path.** Call `apart_factored_denominators` on the new native `Rat` storage. Retain HyperFLINT-shaped linear-pole validation, scaling, ordering tests, and reconstruction tests.
4. **P1 — replace duplicated polynomial primitives.** `Poly` is already a wrapper around Symbolica. Route substitution, evaluation, integration, exact division, GCD, factorization, and resultants directly to the public typed APIs and remove string round trips. Retain only context/error policy and domain-specific proportional normalization.
5. **P1 — implement the pure-Symbolica Euler backend.** Replace HyperFLINT's `msolve` process, file protocol, and output parser with `GroebnerBasis<Zp, _, GrevLexOrder>::new`; retain the DK sector construction, randomized specialization, finite-field constraint-root choice, staircase count, memoization, and conservative voting rules. Benchmark F4 kernel time separately from removed process/serialization overhead.
6. **P2 — integrate Symbolica expression hooks.** **Implemented.** Native derivative and conservative series callbacks are registered for the `Hlog` and `Mpl` heads. Typed word algorithms remain on the integration hot path; arbitrary canonical Atom expressions use Symbolica's chain rule and series engine.
7. **P2 — treat native Atom series and algebraic roots as oracles first.** They are broader than the current code, but Atom conversion and generic algebraic-field machinery may cost more than the specialized rational recurrences/Vieta reductions. Delete the old paths only after correctness and performance gates pass.
8. **P2/P3 — replace string keys and parsers at boundaries.** Use native `Atom`, `PolyVariable`, polynomial/RP `Eq + Hash`, and typed word keys. Keep JSON/C/Python string parsing only as compatibility transport, never as internal CAS.

## Build purity: the selected build has no FLINT path

The root dependency is:

```toml
symbolica = { path = "vendor/symbolica-src", default-features = false,
              features = ["faster_alloc", "integer-gmp", "float-mpfr"] }
```

This is at `Cargo.toml:23-29`. In the vendored manifest, the default feature set includes `native_code_generation`, but defaults are disabled here (`vendor/symbolica-src/Cargo.toml:38-52`). FLINT appears only as the optional dependency behind `flint_benchmarks` and `flint_system_benchmarks` (`vendor/symbolica-src/Cargo.toml:87-92,136-146`). Neither feature is selected. The optional root `python` feature adds Symbolica's `python_export`; that feature's dependency list is at `vendor/symbolica-src/Cargo.toml:61-79` and does not select a FLINT feature.

Both of these checks report that `flint3-sys` is absent:

```text
cargo tree -e features -i flint3-sys
error: package ID specification `flint3-sys` did not match any packages

cargo tree -e features -i flint3-sys --features python
error: package ID specification `flint3-sys` did not match any packages
```

`Cargo.lock` also has no `flint3-sys` package. GMP and MPFR are the selected integer/float arithmetic backends through `rug`; they are not CAS replacements and do not introduce FLINT. Native code generation/JIT is also not selected. The evaluator remains available, but JIT/export modules are gated by `native_code_generation` (`evaluate.rs:7-45`).

Required CI guard: run both `cargo tree` checks above and fail if `flint3-sys` enters either the default or Python graph. Also reject `flint_benchmarks` and `flint_system_benchmarks` in release feature matrices.

## Core expression and symbol APIs

### Atom, AtomView, traversal, and canonical construction

| Hyperbolica need | Exact public Symbolica API | Duplicate now? | Verdict and constraints |
|---|---|---|---|
| Owned expression input | `Atom`, whose variants are `Num`, `Var`, `Fun`, `Pow`, `Mul`, `Add`, `Zero` (`atom.rs:3125-3161`) | The legacy `convert::Expr` is a second expression tree | **WRAP** `Atom` as the only public CAS input. `Expr` may remain a narrow integration IR, not a parser/public algebra type. |
| Borrowed traversal | `AtomView<'a>`, a `Copy` enum over the six nonzero variants (`atom.rs:2390-2408`); sealed `AtomCore` (`atom/core.rs:65-127`) | Some converters manually switch over Atom variants; this is appropriate | **WRAP** using `AtomCore` helpers. Do not clone to traverse. |
| Parse at transport boundary | `Atom::parse(input, namespace, ParseSettings)` (`atom.rs:3277-3315`) | `convert/parse.rs` implements a full tokenizer/parser | **REPLACE** production parsing with `Atom::parse`; keep the custom parser only if a deliberately different legacy grammar is documented and tested. |
| Child/term traversal | `terms`, `children`, and `visitor` (`atom/core.rs:2140-2161,2214-2253`); `AtomTreeIterator` (`id.rs:6309-6355`) | Several scanners/tokenizers/string walkers | **REPLACE** Atom-level scanners. A typed word iterator is domain logic and remains. |
| Symbol/indeterminate discovery | `get_all_symbols`, `get_all_indeterminates`, `contains_symbol`, `contains` (`atom/core.rs:1707-1765`) | `bridge/wire.rs::scan_identifiers`, parser variable collection, and some string-token code | **REPLACE** for Atom inputs. Use `enter_functions` deliberately: `false` treats a function call as an indeterminate; `true` also descends into its arguments. |
| Canonical equality/order/hash | Native `Eq`, `Hash`, `Ord` on `Atom` (`atom.rs:3213-3225`) and `Eq`/comparison on `AtomView` (`atom.rs:2408-2435`) | Canonical strings are used as cache keys in words/regulators/tables | **REPLACE** internal string keys with typed Atom/word keys. Formatting is not an identity representation. |
| Bulk arithmetic/canonicalization | Normalizing operators in `atom/ops.rs`; `Atom::add_many` is an n-way merge and `mul_many` normalizes once (`atom.rs:4457-4492`); `FunctionBuilder::finish` normalizes (`atom.rs:3510-3597`) | Repeated Atom addition/multiplication in output conversion | **REPLACE** bulk folds with `add_many`/`mul_many`; expected to reduce intermediate allocations. Typed polynomial arithmetic should stay typed rather than detouring through Atom. |
| General CAS rewrites | `together`, `apart`, `apart_multivariate`, `cancel`, `factor`, `factor_complex`, `factor_in_extension` (`atom/core.rs:395-520`) | Some boundary code re-parses and normalizes rational text | **WRAP** for general user expressions; use typed polynomial/RP APIs in hot kernels. `apart_multivariate` explicitly computes a Gröbner basis and may be slow (`atom/core.rs:426-450`). |

### Centralized symbols and hooks

`initialize!` registers a module-scope `StateInitializer` with dependency ordering (`state.rs:203-253`); the global mappings and thread-local recyclable workspace are at `state.rs:266-282`. `symbol!` builds or fetches symbols through `SymbolBuilder` (`atom.rs:4021-4122`), while `get_symbol!` is lookup-only and returns `Option<Symbol>` (`atom.rs:4215-4240`). `symbol_group!` is available for mutually dependent hooks (`atom.rs:4242-4297`).

`SymbolBuilder` supplies the exact public extension points Hyperbolica needs:

- normalization callback: `with_normalization_function(Fn(AtomView, &mut Settable<Atom>))` (`atom.rs:917-938`);
- partial derivative callback: `with_derivative_function(Fn(AtomView, usize, &mut Settable<Atom>))` (`atom.rs:988-1010`);
- singular Laurent callback: `with_series_function(for<'a> Fn(&'a [Series<AtomField>]) -> Option<(Atom, Atom)>)` (`atom.rs:1023-1034`);
- numerical evaluator registration: `with_evaluation_info(EvaluationInfo)` (`atom.rs:1050-1055`).

The derivative callback returns the partial derivative with respect to argument `index`; Symbolica's derivative engine multiplies it by that argument's derivative and sums the terms, i.e. applies the chain rule (`derivative.rs:181-230`). A Hyperbolica hook must not multiply by the argument derivative itself.

Current status: **P0 implemented.** `src/symbols.rs` owns all fixed heads in one `initialize!` block, `heads()` performs lookup-only `get_symbol!(...).expect(...)` calls, and its constructors produce registered `MZV(...)`, `Wm(i)`, `Wp(i)`, `WmOverWp(i)`, `sqrt_disc(i)`, `delta(...)`, and `Period(...)` atoms. MZV reduction/expansion/period contexts, algebraic-letter contexts, the concrete-`Atom` Rust API, and both direct/prepared Python paths now reserve and locate these indeterminates structurally through `PolyCtx::from_indeterminates` and `index_of_indeterminate`. Non-symbol function indeterminates are retained unchanged.

Legacy spellings (`mzv_2`, `Wm_1`, and related names) are confined to generated-table and JSON/string compatibility data. `src/symbols/legacy.rs` imports and exports them with Symbolica's structural `replace_map`; bridge output applies the reverse translation, retaining the HyperFLINT wire format. A repository source gate for dynamic special-symbol construction returns no matches:

```sh
rg -n '(PolyCtx::new|symbol!|Symbol::parse)[^\n]*"(mzv_[m0-9]|Log2"|Wm_[0-9]|Wp_[0-9]|WmOverWp_[0-9]|sqrt_disc_[0-9])' \
  src --glob '!bridge/**' --glob '!symbols/legacy.rs'
```

Verdict: **WRAP, complete for P0.** The representation is polynomial-compatible because `PolyVariable` supports `Symbol`, `Function(Symbol, Atom)`, `Power(Atom)`, and `Temporary` and implements conversion from function/power Atoms (`poly.rs:738-848`). Benchmark context construction and large MZV reductions as part of the remaining performance gates.

### Implemented `Hlog`/`Mpl` expression hooks (P2)

The hook API and its execution semantics were re-audited before registration.
`SymbolBuilder::with_derivative_function` accepts a partial derivative callback
and `with_series_function` accepts a transform from argument series to a
`(singular, regularized)` Atom pair (`atom.rs:988-1034`). The derivative engine
multiplies a callback result by the derivative of that argument and sums over
arguments (`derivative.rs:181-230`); Hyperbolica's callback therefore never
adds its own chain-rule factor. An unset `Settable` preserves Symbolica's
generic `der(...)` form. The series engine expands every argument first, calls
the custom transform, and expands/multiplies the returned pair
(`derivative.rs:445-505`). Public `Series<AtomField>` precision, coefficient,
term, variable, and Atom-conversion accessors are at `poly/series.rs:393-685`
and `poly/series.rs:1434-1464`.

The canonical normalized encodings are:

- `Hlog(z,a1,...,an)`, with `Hlog(z)` normalized to the exact empty-word unit;
- `Mpl(n1,...,nd,z1,...,zd)`, requiring even arity and positive exact integer
  indices, with `Mpl()` normalized to the exact depth-zero unit.

Symbolica flattens an `arg(...)` passed to a function during normalization
(`normalize.rs:835-849`), so the public compatibility spellings
`Hlog(z,arg(a1,...))` and `Mpl(arg(n1,...),arg(z1,...))` reach those same
shapes. `convert/atom.rs` also descends through a surviving raw `arg` wrapper
when collecting rational indeterminates; it never promotes the complete list
call to one polynomial variable.

The one central `initialize!` block registers normalization, derivative, and
series callbacks in `src/symbols.rs:32-55`. Exact partial Hlog and MPL
formulas are in `src/symbols/hooks/derivative.rs`; coincident Hlog letters or
endpoints, zero divisors, non-positive MPL indices, and malformed MPL arity
leave the callback unset. Consequently unsupported singular calls remain
generic instead of acquiring an incorrect rational derivative. Empty,
depth-one, moving-letter/argument, depth-greater-than-one, and nested-argument
chain-rule tests compare the native Atom derivative with `diff_hlog` and
`diff_mpl` structurally after exact cancellation.

Series registration in `src/symbols/hooks/series.rs` deliberately covers only
regimes for which the retained typed algorithms provide the requested finite
precision exactly:

- an Hlog endpoint with positive order and letters that are either exact zero
  or regular with nonzero limit uses `hlog_zero_expand`; rational argument
  series are converted structurally to a temporary typed context without
  parsing or formatting;
- an MPL whose last argument has positive order, with no argument poles, uses
  the finite defining `mpl_sum`; a depth-greater-than-one *first-only* zero
  limit is not registered because bounding the outer summation index does not
  generally bound the expansion order in the first argument;
- a regular depth-one MPL is exactly the classical polylogarithm. HyperFLINT's
  type contract states `Mpl({n},{z}) = Li_n(z)`, while Symbolica documents its
  public `polylog(s,z)` as the principal branch (`transcendental.rs:2059-2064`)
  and supplies exact normalization/derivative rules
  (`transcendental.rs:486-507,3458-3514`). The zero-limit defining sum is used
  before this delegation, avoiding the removable `polylog(n,z)/z` derivative
  at the origin.

Hlog argument poles/infinity, nonzero vanishing letters, depth-greater-than-one
MPL logarithmic singularities, malformed shapes, insufficient finite series
data, and requested hook orders above 512 return `None`. The cap is a
performance/failure boundary, not an approximation: Symbolica falls back to
its generic series behavior. Tests cover zero and regular Hlog series,
depth-one and depth-two MPL defining sums, the native depth-one polylog oracle,
the weight-one logarithmic singularity, malformed arity, first-only MPL zero,
and argument-pole deferral. Runtime execution remains part of the pending
licensed serial gate; all hook tests are included in the compile/no-run gate.

### Structural equality, hashing, and cache keys

The P2/P3 key migration was re-audited against the complete public equality and
ordering surface before changing the LR caches. `AtomView` compares its
canonical encoded data and implements structural `Ord` and `Hash`
(`atom.rs:2408-2435,2655-2672`); owned `Atom` delegates `Eq`, `Hash`, and `Ord`
to that view (`atom.rs:3213-3225`). `PolyVariable` derives structural
`Eq + Hash + Ord`, including owned function/power Atoms (`poly.rs:738-758`).
`MultivariatePolynomial` implements structural `Eq + Hash` over coefficients,
exponents, and the nonconstant variable map
(`poly/polynomial.rs:1647-1687`). Symbolica itself uses those polynomial values
directly as `HashMap`/`HashSet` keys, for example in the algebraic-root cache
and Gröbner condition collection (`domains/algebraic.rs:3768-3794` and
`poly/groebner.rs:2090-2140`). `RationalPolynomial` likewise derives
`Eq + Hash` from its canonical numerator and denominator
(`domains/rational_polynomial.rs:90-95`).

There is deliberately no public `Ord` implementation for
`MultivariatePolynomial`. The public `InternalOrdering` trait exists
(`lib/numerica/src/domains.rs:40-44`), but its polynomial implementation
explicitly has a TODO for different variable maps and compares only exponent
and coefficient arrays (`poly/polynomial.rs:1689-1695`). Verdict: **do not use
it as cross-context identity or as a wire-order contract**. Use full native
`Eq + Hash` for lookup, preserve first-encounter/ID ordering internally, and
perform the legacy lexical ordering only after formatting at the JSON boundary.

Current status: **P2/P3 implemented for LR search, LR scan, and factor-table
hot paths.** `Poly` now implements `Hash` consistently with its existing
context-sensitive `Eq`: it hashes the ordered structural `PolyVariable` map
exactly once, followed by Symbolica's canonical coefficient and exponent
arrays. `Rat` applies the same rule to its canonical numerator and denominator
payloads, and composite Word/SymCoef/slice hashes reuse payload-only helpers so
they do not revisit one large context for every entry. Diagnostic
`PolyCtx::vars()` spellings are presentation metadata, not ring identity.
The reduction step cache stores full `(variable, Vec<Poly>)` keys in collision
buckets; its `u64` digest only selects a bucket. The proportional dedup,
intersection, carry/minted ledgers, factor cache, singularity collector, and
factor-table interner all confirm complete `Poly` equality. Forced-collision
tests verify that unequal polynomials, variables, and ordered slices sharing a
digest never alias. LR/factor-table wire strings are now created only in
`bridge/lr.rs`; the bridge restores the historical lexical pair orientation
and sign after formatting, so internal structural ordering does not alter the
protocol. `OrderVerifyResult` retains its optional blocking letter as a
structural `Poly`, and singularities, blockers, and factor-table interned
polynomials all pass through `bridge::wire_poly`; registered `Wm`/`Wp`/`MZV`
function indeterminates therefore recover their legacy spellings before the
lexical wire comparison is performed.

An audit of every root export and call site also found two unused public
wrappers, `RatAddKey` and `ReduceKey`, whose derived `Eq + Hash` compared only
digest fields. They have been deleted together with the public
`context_signature`, `poly_signature`, and `rat_signature` exports.
`poly_bucket_digest` and the general `structural_bucket_digest[_by]` machinery
are crate-private, explicitly described as bucket accelerators rather than
semantic keys, and consumed only by structures that compare complete typed
values inside every matching bucket. Their `StableFnv1aHasher` has a fixed
FNV-1a definition and fixed-width little-endian integer events, so bucket and
seed behavior does not inherit randomized or standard-library hasher changes.
Namespaced-context and forced-collision regressions cover this invariant.

The word/primitive key path has now received the same structural treatment.
The complete relevant public surface was checked again: `PolyVariable`
derives structural `Hash + PartialEq + Eq` across symbol, function-Atom,
power-Atom, and temporary variants (`poly.rs:745-760`), and canonical
`RationalPolynomial` derives `PartialEq + Eq + Hash` over its native numerator
and denominator (`domains/rational_polynomial.rs:91-96`). Those polynomials in
turn use the coefficient/exponent arrays plus the structural variable map for
nonconstants (`poly/polynomial.rs:1647-1687`); constants intentionally compare
without a variable map. Hyperbolica's `Rat::Eq` is stricter because it also
requires the complete ordered `PolyVariable` map, so `Rat::Hash` prefixes that
exact structural context before hashing the native rational polynomial
(`src/core/rat/traits.rs`). `Word` consequently derives full ordered
structural `Eq + Hash` (`src/symbols/word.rs`). Because `Rat` also owns lazy
compatibility views, the primitive accumulator does not place `Word` directly
in a hash-map key: it maps each structural digest to candidate row indices and
then resolves every match with full `Word` equality against the complete
source value stored in that row (`src/integrator/primitive.rs`). Hash digests are
therefore never treated as equality. Forced-collision, canonical-value,
cross-context, and first-encounter-order regressions cover the contract.

The transform, period-fibration, and integration-step hot paths received the
same audit before their formatted keys were removed. The decisive public
Symbolica contracts remain `AtomView` canonical-byte `Eq + Hash + Ord`
(`atom.rs:2408-2435,2655-2672`), owned `Atom` delegation
(`atom.rs:3213-3225`), structural `PolyVariable` equality and hashing across
symbol/function/power variants (`poly.rs:745-760`), canonical polynomial
`Eq + Hash` (`poly/polynomial.rs:1647-1687`), and rational-polynomial
`Eq + Hash` (`domains/rational_polynomial.rs:91-96`). There is still no safe
public cross-context polynomial `Ord`; `InternalOrdering` retains its
different-variable-map TODO (`poly/polynomial.rs:1689-1695`). Verdict:
**REPLACE formatted semantic keys with collision buckets, RETAIN formatting
only for legacy presentation order.**

Transform collection now resolves a digest bucket with complete canonical
`RegKey` equality; result rows retain and compare complete `RegulatorSym` and
`Word` values; recursive and integration-step caches retain and compare the
full `(variable, Word)` and `(variable, Vec<Word>)` inputs. Repeated-log
grouping compares the complete `Rat` letter. Fibration accumulators retain and
compare complete `RegKey` values. Symbolic regulator equality additionally
checks the ordered structural `PolyVariable` map before comparing every
symbolic monomial. Diagnostic names never enter identity. No `u64` is an
equality proof. Injected-collision tests cover each bucket family; namespaced
equal-spelling contexts prove context identity is decisive; reversed-input
regressions preserve canonical output. Criterion cases exercise regulator
collection, period accumulation, and shared integration-step spines in
`benches/semantic_keys.rs`.

`Word::content_key` remains only as an explicitly legacy
compatibility/presentation spelling. Its letters are rendered from native
`Rat::to_atom()` values, so compatible contexts built through different
constructors agree without consulting diagnostic names. Commutative `RegKey`
products canonicalize directly with a total structural comparator and do no
formatting on that hot path. Final regulator/fibration row ordering still uses
the established lexical presentation key with structural collision ties, and
the JSON bridge rebuilds lexical factor/term order from the strings it
actually emits. The key is no longer used by the primitive, transform,
period, or integration-step semantic caches and must not be introduced into
new lookup paths.

Context and algebraic-letter interning now follow the same rule. The context
interner hashes the ordered structural `PolyVariable` list, retains weak
contexts in digest buckets, and confirms that complete map before reusing an
allocation. Diagnostic names do not split the same mathematical ring across
the symbol and Atom-indeterminate constructors, while structural namespaces
remain decisive. The algebraic-letter registry hashes the complete
context-sensitive `Poly` value together with `var_idx`, retains only entry IDs
in each bucket, and confirms `(var_idx, Poly::Eq)` before deduplication. Neither
path uses a digest as identity, and first-seen context allocations and
one-based algebraic-letter IDs remain stable. Namespace-alias and injected
digest-collision regressions cover both paths
(`src/core/context_interner.rs`,
`src/algebra/algebraic_letters/registry.rs`).

Formal delta generators now follow the same context rule. `SymMonomial` and
its split representation store `BTreeMap<usize, i32>` indices into the ordered
native context instead of diagnostic variable strings. Public construction
validates every index, and Atom output calls `PolyCtx::variable_atom` directly,
so two namespaced variables that both print as `x` remain distinct. The legacy
JSON adapter translates registered legacy names to native Atoms, compares the
actual emitted wire spellings, and rejects unknown or ambiguous aliases. It
maps validated indices back through `PolyCtx::variable_atom` plus the legacy
special-name adapter only when producing historical wire text. Delta factors
and symbolic monomial summands recover upstream lexical string ordering at
that boundary, independent of context-index order. Cross-namespace Atom-output,
registered-special, reversed-context, and ambiguous-wire regressions cover
both sides of that boundary.

This design deliberately wraps Symbolica's public contracts instead of
inventing a parallel key representation: owned `Atom` delegates `Eq + Hash`
to canonical `AtomView` data (`atom.rs:2408-2435,2655-2672,3213-3225`),
`PolyVariable` derives structural `Eq + Hash` including owned function/power
Atoms (`poly.rs:745-760`), and `MultivariatePolynomial` compares and hashes
canonical coefficient/exponent arrays plus nonconstant variable maps
(`poly/polynomial.rs:1647-1687`). Because native constant-polynomial equality
intentionally omits its variable map, Hyperbolica's `Poly::Eq + Hash` prefixes
the complete context; that wrapper, rather than the native polynomial alone,
is the algebraic-letter identity.

## Polynomial and rational-function APIs

### Atom conversion

| Conversion | Exact API and bounds | Migration |
|---|---|---|
| Atom to polynomial | `AtomCore::try_to_polynomial<R: EuclideanDomain + ConvertToRing, E: Exponent>(&R, variables)` (`atom/core.rs:1277-1352`) | **WRAP** in `Poly::from_atom`. Non-polynomial components can become indeterminates; Hyperbolica must compare the returned variable map to its explicit context. |
| Atom to polynomial with Atom coefficients | `to_polynomial_in_vars<E: Exponent>` (`atom/core.rs:1354-1408`) | Use only when selected indeterminates should be variables and everything else should stay in `AtomField`. It is not a replacement for a pure-Q `Poly`. |
| Atom to canonical RP | `try_to_rational_polynomial<R, RO, E>` (`atom/core.rs:1410-1509`). Input `R: EuclideanDomain + ConvertToRing`; output `RO: EuclideanDomain + PolynomialGCD<E>`; the target RP needs the appropriate `FromNumeratorAndDenominator` implementations | **WRAP, implemented** once at the input boundary: `Rat::from_atom` stores the resulting integer RP directly. |
| Atom to factorized RP | `try_to_factorized_rational_polynomial` (`atom/core.rs:1511-1593`) additionally requires output polynomial `Factorize` support | Selective **WRAP** only; see factorized-RP caveats below. |
| Typed polynomial/RP to Atom | `MultivariatePolynomial::to_expression[_into]` (`poly.rs:2277-2363`) and `RationalPolynomial::to_expression[_into]` (`poly.rs:2436-2464`) | **REPLACE** custom formatting/reparse conversion. The `_into` forms permit recyclable output storage. |

### MultivariatePolynomial

`MultivariatePolynomial<F,E,O>` is a sparse, sorted, expanded representation (`poly/polynomial.rs:742-753`) with a shared ring/variable context (`poly/polynomial.rs:789-872`). Native structural equality and hashing include coefficients, exponents, and the nonconstant variable map (`poly/polynomial.rs:1647-1687`). `Poly` already stores `MultivariatePolynomial<RationalField,u16>` (`src/core/poly.rs:11-18`), so most `Poly` work is already Symbolica-backed; the right migration is to thin the wrapper, not introduce Atom arithmetic.

| Current `Poly` operation | Native operation | Duplicate? | Verdict / performance note |
|---|---|---|---|
| zero/one/generator/int, term count, degree, bounds, coefficient | constructors and accessors (`poly/polynomial.rs:868-1120`), `degree`, `degree_bounds`, `lcoeff`, `derivative`, `coefficient` (`poly/polynomial.rs:2101-2239`) | Wrapper/validation only | **WRAP**. Keep bounds checking and Hyperbolica's zero-degree convention (`-1`) explicit. |
| add/sub/mul/pow | native operators; `.pow(usize)` (`poly/polynomial.rs:3236`) | No independent CAS | **WRAP**. |
| exact division and quotient/remainder | trait-selected `.try_div_exact` (`poly/polynomial.rs:5008-5012`); `quot_rem` (`poly/polynomial.rs:5088-5115`); generic `.try_div` (`poly/polynomial.rs:5139-5165`) | Thin wrapper | Prefer `try_div_exact` because the coefficient domain selects the algorithm; preserve division-by-zero errors. |
| GCD/factor | `.gcd` (`poly/gcd.rs:3560`) backed by `PolynomialGCD` (`poly/gcd.rs:7111+`); `Factorize::{square_free_factorization,factor,is_irreducible}` (`poly/factor.rs:2369-2378`) with Z/Q/finite-field implementations | Thin wrapper | **WRAP** native only. Do not build a second GCD/factor algorithm. |
| substitution | `.replace(var, coefficient)`, `.replace_all`, `.replace_with_poly` (`poly/polynomial.rs:2556-2717`) | Former string-valued `Poly::substitute_rational` | **REPLACE, implemented** with typed `Rational`/`Integer`; no core formatting/reparse. |
| evaluation | `.evaluate`, `.evaluate_with_coeff_map`, `.replace_all` (`poly/polynomial.rs:2651-2705`) | Former string-valued `Poly::evaluate_all` | **REPLACE, implemented** with typed exact return values. |
| variable remap/transplant | `.rearrange` and `.rearrange_with_growth` (`poly/polynomial.rs:3181-3229`) | `Poly::transplant` is custom | **WRAP** native for bijective/permutation maps. Retain explicit validation/merge logic if Hyperbolica allows many-to-one maps; do not assume the native operation has that policy. |
| split as polynomial in one/many variables | `.to_univariate`, `.to_univariate_polynomial_list`, `.to_multivariate_polynomial_list` (`poly/polynomial.rs:3306-3425`) | `Poly::coefficient_of` scans terms | **RETAIN** the direct sparse coefficient query until benchmarked. `to_univariate_polynomial_list` allocates a vector spanning min-to-max degree (`poly/polynomial.rs:3368-3401`), so it can lose badly on sparse high-degree inputs. |
| polynomial antiderivative | `.integrate(var)` for `F: Field` (`poly/polynomial.rs:6233-6255`) | `primitive.rs` manually loops polynomial coefficients | **REPLACE** the polynomial-part loop. |
| proportional canonical form | content/sign helpers, but no API with HyperFLINT's exact projective convention | Domain-specific | **RETAIN** `canonical_proportional_form`, implemented with native content/leading-coefficient operations; test invariance under nonzero scalars. |
| reciprocal-variable reversal | `.reverse()` exists, but reverses every variable, not a selected target | Current selected-variable reciprocal substitution is different | **RETAIN** the selected-variable algorithm or rebuild it from typed term iteration. Do not substitute global `.reverse()`. |

### Implemented typed polynomial operations (P1)

Current status: **P1 implemented** for substitution, evaluation, polynomial integration, and context rearrangement. `Poly::substitute_rational` and `Poly::substitute_integer` call public `MultivariatePolynomial::replace`; `Poly::{evaluate_rational,evaluate_integer}` call `replace_all`; and `Poly::integrate` wraps the public field-only `MultivariatePolynomial::integrate` while checking the variable index and `u16` exponent overflow first. The exact implementation bounds are `F: Ring, E: PositiveExponent` for `replace`/`replace_all` (`poly/polynomial.rs:2474-2743`) and `F: Field, E: PositiveExponent, O: MonomialOrder` for `integrate` (`poly/polynomial.rs:6233-6257`). The vendored regression tests exercise sparse Horner replacement and cancellation (`poly/polynomial.rs:7041-7054`) and last-active-variable replacement against independent term evaluation (`poly/polynomial.rs:8200-8255`); the univariate integration test verifies derivative/integral inversion (`poly/univariate/tests.rs:137`).

`Rat::substitute_rational` starts from the native integer `RationalPolynomial` numerator and denominator, maps their coefficients to `Q`, invokes typed `replace`, and constructs the normalized integer RP directly. `Rat::substitute_integer` remains entirely in the integer polynomial ring. Neither path requests the lazy compatibility `Poly<Q>` views. Full rational evaluation uses the public mapped field operation on the two native polynomials with an explicit zero-denominator check; full integer evaluation uses native integer `replace_all` before constructing one `Rational`. `RationalPolynomial::evaluate_with_coeff_map` is public for `R: Ring`, `U: Field` (`domains/rational_polynomial.rs:730-746`) and its vendored finite-field test is at `domains/rational_polynomial.rs:1774-1782`. A complete public-source/test/example search found no partial-variable replacement method on `RationalPolynomial`, so the typed numerator/denominator lift is **RETAIN as the minimal adapter**, not an independent CAS algorithm.

Substitution/evaluation scalar strings are now confined to the JSON compatibility parser in `src/bridge/wire.rs`; production `Poly`/`Rat` substitution and evaluation accept Symbolica `Rational`/`Integer` values. The explicit `Poly::parse`/`Rat::parse` constructors remain legacy/test conveniences, while the production integration façade accepts `Atom`. `integrator/primitive.rs` delegates its polynomial part to `Rat::integrate_polynomial_part`, which lifts the native integer numerator/denominator once and calls `MultivariatePolynomial::integrate` without materializing compatibility views; the simple-pole/word logic and denominator policy remain Hyperbolica-specific.

`Poly::transplant` uses public `rearrange_with_growth` (`poly/polynomial.rs:3192-3229`) whenever the requested map is exactly the structural identity-induced permutation/growth/drop map. The explicit sparse adapter is **RETAIN** for renaming and many-to-one identification because native rearrangement only relocates existing distinct variables; the adapter sums identified exponents, merges colliding monomials, and reports exponent overflow. `coefficient_of` is also **RETAIN** as a direct sparse scan: public `coefficient` returns only one complete monomial coefficient (`poly/polynomial.rs:2210-2223`), while `to_univariate_polynomial_list` allocates the min-to-max degree span (`poly/polynomial.rs:3353-3401`) and `to_multivariate_polynomial_list` materializes the full split. Focused unit/property tests cover sparse rational/integer substitution, complete evaluation, poles, context/arity/index errors, integration round trips, sparse coefficient gaps, native-view laziness, native rearrangement, and many-to-one transplantation. Criterion cases live under `polynomial/typed_exact_ops` and `rational/typed_exact_ops` in `benches/core_algebra.rs`.

### Resultant and discriminant

`UnivariatePolynomial<F: EuclideanDomain>` exposes Brown's PRS resultant and the default Lazard-Ducos resultant (`poly/resultant.rs:91-130`). For coefficients that are integer or rational multivariate polynomials, specialized public CRT reconstruction is available (`poly/resultant.rs:581-610,735-761`); over a coefficient field there is `resultant_euclidean` (`poly/resultant.rs:789-810`). The default documents why Ducos is attractive for multivariate-polynomial coefficients (`poly/resultant.rs:101-108`).

`Poly::resultant` already calls `to_univariate(variable).resultant(...)` (`src/core/poly.rs:452-460`), so it is already using the improved vendored code. **WRAP** it. Benchmark the default against `resultant_crt()` on LR workloads rather than assuming one wins; record degree/term/coefficient-size regimes.

No public `discriminant` method was found in the selected snapshot. Compose it from native derivative, resultant, and leading coefficient:

```text
disc_x(f) = (-1)^(n(n-1)/2) * Res_x(f, d f/dx) / lc_x(f).
```

The current implementation returns only `Res/lc` (`src/core/poly.rs:462-470`), so odd sign-parity cases are wrong under the conventional definition. For example, a quadratic needs the negative of `Res/lc`. **RETAIN** a small wrapper but add the sign. If existing LR logic only uses factors up to proportionality, say so in that internal call site; do not expose the signed quotient as a conventional discriminant.

No public exact-square predicate was found. The helper named `exact_square_root` in `poly/factor.rs` is private. Retain the current factorization-based test, or express it through public `Factorize` and even multiplicities; benchmark it.

### RationalPolynomial: implemented `Rat` storage

`RationalPolynomial<R,E>` publicly stores numerator and denominator polynomials (`domains/rational_polynomial.rs:90-95`). For `IntegerRing`, construction clears Q coefficient denominators and normalizes through `FromNumeratorAndDenominator` (`domains/rational_polynomial.rs:357-445`). This invariant is **not** the current Q-pair invariant: it cancels a requested GCD and makes the denominator's leading sign positive, but does not make the integer denominator monic. Public structural tests must therefore compare canonical native RPs/Atoms, not expect the old monic-Q numerator/denominator pair.

Native RP functionality and bounds:

- `inv`, `pow`, and `gcd` require `R: EuclideanDomain + PolynomialGCD<E>` and a matching `FromNumeratorAndDenominator` (`domains/rational_polynomial.rs:526-564`). The native `pow` contains an explicit binary-exponentiation TODO and multiplies `e` times (`domains/rational_polynomial.rs:540-555`); retain a wrapper that powers numerator/denominator with polynomial exponentiation by squaring, including signed exponents and zero checks.
- `evaluate` exists only for `R: Field` (`domains/rational_polynomial.rs:713-720`). Integer-backed RP evaluation must use `evaluate_with_coeff_map<U: Field>` (`domains/rational_polynomial.rs:722-737`).
- addition computes a denominator GCD, chooses a smaller multiplication arrangement, and removes the residual common factor (`domains/rational_polynomial.rs:1051-1100`); multiplication cross-cancels numerator/denominator pairs (`domains/rational_polynomial.rs:1136-1177`). These duplicate `Rat::try_add` and `Rat::try_mul` (`src/core/rat.rs:151-189`).
- native quotient-rule `derivative` is public (`domains/rational_polynomial.rs:1193-1222`) and duplicates `Rat::derivative`.
- when rational functions are used as a polynomial coefficient field, its `PolynomialGCD` implementation asserts that at most one outer variable is active (`domains/rational_polynomial.rs:1007-1029`). Do not generalize that coefficient-field use to multivariate outer GCDs.

Verdict: **REPLACE, implemented.** `Rat` stores `RationalPolynomial<IntegerRing,u16>` behind a shared `Arc` and wraps panicking preconditions with typed checks. This gives one canonical representation, native small/large multiplication choices, and direct partial fractions. The compatibility `&Poly<Q>` numerator/denominator views are projected once into a shared `OnceLock`, never on every access; native arithmetic, substitution, evaluation, differentiation, powers, and partial fractions bypass them. Changed structural normalization is tested by native RP/Atom value rather than old numerator/denominator shapes. Small-Q and shared-denominator cases remain benchmark gates.

### Partial fractions and rational integration

`RationalPolynomial` exposes:

- `apart(var) -> Vec<Self>` (`domains/rational_polynomial.rs:1246-1259`);
- `apart_factored_denominators(var) -> Vec<(Self, Self, usize)>`, preserving each irreducible denominator base and power (`domains/rational_polynomial.rs:1261-1348`);
- `apart_multivariate()`, with bounds `R: EuclideanDomain + UpgradeToField + PolynomialGCD`, upgraded field `Echelonize`, and polynomial `Factorize` (`domains/rational_polynomial.rs:1351-1373`); its implementation uses a Gröbner basis (`domains/rational_polynomial.rs:1400-1429`);
- `integrate(var) -> RationalIntegral`, separating rational and logarithmic/root-sum parts (`domains/rational_polynomial.rs:97-122,1477-1505`).

`src/algebra/partial_fractions.rs` now calls `apart_factored_denominators` directly on native `Rat` storage and retains the HyperFLINT output adapter, nonlinear-factor error, pole scaling, ordering, and reconstruction validation. Native regression coverage includes the factored-denominator test at `domains/rational_polynomial.rs:2065-2066`; Hyperbolica adds repeated/nonmonic/parameter/polynomial-part/zero/nonlinear reconstruction cases.

Do **not** immediately replace `integrator/primitive.rs::integrate_ii` with `RationalPolynomial::integrate`. Hyperbolica must emit word/log objects with its branch and regularization conventions, whereas `RationalIntegral` emits ordinary logs or algebraic root sums. More importantly, the native LRT/subresultant implementation contains the source comment “this may be wrong” (`domains/rational_polynomial.rs:1641-1644`). Use native integration as an **ORACLE/guarded experiment** only: differentiate every result and reconstruct the input over randomized exact examples before enabling it. The safe immediate change is native polynomial `.integrate` for the polynomial part, then the existing simple-pole word construction.

### FactorizedRationalPolynomial

The native factorized type exposes numerator/coefficient/factor storage (`domains/factorized_rational_polynomial.rs:28-77`), Q→Z and integer normalization/factorization (`domains/factorized_rational_polynomial.rs:397-543`), `inv`, `pow`, `gcd`, field/mapped evaluation (`domains/factorized_rational_polynomial.rs:714-868`), arithmetic, and `apart` (`domains/factorized_rational_polynomial.rs:1481-1509`). It is not yet a safe wholesale replacement for `FactoredRat`:

- `InternalOrdering::internal_cmp` is `todo!()` and will panic if invoked (`domains/factorized_rational_polynomial.rs:79-83`);
- construction has a TODO to fuse equal factors (`domains/factorized_rational_polynomial.rs:488-500`);
- inversion factors the numerator and is explicitly documented expensive (`domains/factorized_rational_polynomial.rs:719-736`);
- `pow` is repeated multiplication with a binary-exponentiation TODO (`domains/factorized_rational_polynomial.rs:740-761`);
- no public derivative was found.

Verdict: selective **WRAP/ORACLE**, not replacement. Prototype native storage/add/mul/apart behind benchmarks, but retain Hyperbolica's equal-factor merging, signed fast powers, derivative, peel/materialization policies, errors, and any ordering-dependent cache keys.

## Series APIs

`Series<F: Ring>` is a Puiseux series with rational exponents, expansion point, shift, truncation order, and ramification (`poly/series.rs:140-203`). `SeriesDepth::{Absolute,Relative}` is public (`poly/series.rs:140-175`). Public construction/access includes `new`, relative/absolute order, expansion point, variable, coefficient, and term iteration (`poly/series.rs:231-280,393-415,530-685`). Generic ring series support addition and multiplication (`poly/series.rs:912-1077`), but `/` is implemented specifically for `Series<AtomField>` (`poly/series.rs:1079-1093`), not for an arbitrary rational-function coefficient field. `Series<AtomField>` also exposes `exp`, `log`, `sin`, `cos`, series/series power, rational power, and Atom conversion (`poly/series.rs:1130-1464`).

The expression entry point is `AtomCore::series(variable, expansion_point, depth) -> Result<Series<AtomField>, SeriesError>` (`atom/core.rs:700-731`). Built-in and custom functions participate through `SymbolBuilder::with_series_function`; the engine invokes those hooks in `derivative.rs:376-530`. Native tests include general series tests at `derivative.rs:908+` and singular transcendental tests in `transcendental.rs`.

Migration verdicts:

- `series/laurent.rs::series_expansion` duplicates rational-series functionality semantically, but its exact coefficient recurrence stays in `Rat`/`Poly` and may avoid substantial Atom normalization. **RETAIN + ORACLE** against `Rat::to_atom().series(...)`; benchmark sparse/high-order/pole cases before replacing.
- `substitute_variable_reciprocal` and `coefficient_at_zero` have Hyperbolica-specific selected-variable and admissibility semantics. **RETAIN**, but implement with typed native terms; Symbolica's polynomial `reverse()` is not a selected-variable substitute.
- `series/expansions.rs`, `hlog_series.rs`, `mpl_series.rs`, and `mpl_sum.rs` are special-function algorithms absent from Symbolica's public API. **RETAIN**. The registered Hlog/MPL series callbacks now route only their exact supported regimes into these routines, while the typed integration path remains hot.
- Use `Series::coefficient`/`terms` for native-oracle comparison. Do not attempt generic native RP-series division without a public implementation.

## Algebraic roots and extensions

### Univariate exact roots

For a univariate polynomial over `Q`, `root(index)`, `isolate_roots`, and `isolate_real_roots` return certified `IsolatedRoot` values with multiplicities (`poly/univariate/roots.rs:1208-1263`). Exact rational real intervals are public (`poly/univariate/roots.rs:2124-2183`), and approximate complex roots are available (`poly/univariate/roots.rs:2239-2269`). Exact complex-rational coefficient polynomials have parallel `root`/`isolate_roots` methods (`poly/univariate/roots.rs:2434-2475`). `IsolatedRoot` exposes its defining polynomial, exact disk, canonical index, Atom representation, refinement, and location classification (`poly/univariate/roots.rs:682-805`). The extensive root test suite begins at `poly/univariate/roots/tests.rs:24`.

These APIs apply to exact Q or exact complex-rational coefficients, not unresolved parameter coefficients. For Hyperbolica's parameter-dependent denominator factor `c1*x+c0`, the best path remains the exact typed formula `-c0/c1` after native factorization. **RETAIN** `linear_factors` as a HyperFLINT-shaped adapter, but ensure factorization and coefficient extraction are native. For parameter-free nonlinear roots, **WRAP** `IsolatedRoot` rather than inventing isolation/indexing.

### Formal parameter roots and algebraic fields

`Root<Q>` converts from an Atom polynomial, optionally with an explicit variable, simplifies to the selected irreducible factor, and converts back to an Atom (`domains/algebraic.rs:3510-3597`). `Root<RationalPolynomialField<IntegerRing,u16>>` does the same over `Q(parameters)` (`domains/algebraic.rs:3600-3750`); its branch index is formal until parameters are specialized, and its simplifier has explicit linear/quadratic and limited cubic forms.

Native field types are `AlgebraicExtension<R>`, `AlgebraicQuotient<R>`, `Root<R>`, and `AlgebraicContext` (`domains/algebraic.rs:150-325`). `AlgebraicContext::{from_atom,from_generators,adjoin_root,extend,convert_atom}` is public (`domains/algebraic.rs:690-705,801-830,907-976`). `AlgebraicQuotient::adjoin_formal` and `AlgebraicExtension::adjoin_formal` collapse towers without selecting an analytic embedding (`domains/algebraic.rs:2427-2461,2546-2562`). Constructors require a monic irreducible polynomial but document that irreducibility is not checked (`domains/algebraic.rs:1567-1586,2176-2185`).

`algebra/algebraic_letters.rs` overlaps this machinery but also implements upstream Wm/Wp allocation, names, ratio contraction, square-root back-substitution, and Vieta recurrence. Verdict: first **REPLACE** dynamic names with the fixed calls `Wm(i)`, `Wp(i)`, `WmOverWp(i)`, `sqrt_disc(i)`. Then **ORACLE** the Vieta recurrence against `Root<Q(parameters)>`/`AlgebraicQuotient` reduction. Retain the table and naming/branch contract until benchmarks show that general quotient-field reduction is no slower and all upstream conventions match.

### Implemented quadratic-letter introduction path

The public API was re-audited before enabling quadratic poles in the
integration pipeline. `Root<RationalPolynomialField<IntegerRing,u16>>`
constructs and simplifies formal roots over `Q(parameters)`
(`domains/algebraic.rs:3600-3750`), including the two quadratic branches, but
it is a root descriptor and does not expose partial fractions. Expression
`factor_in_extension` converts through `AlgebraicContext`, whose coefficient
field is an algebraic extension of `Q` (`domains/algebraic.rs:1360-1402`); its
tests cover constant generators such as `sqrt(2)` and `root(x^3-2,2)`
(`domains/algebraic/tests.rs:331-414`), not unresolved parameter-dependent
roots. The public polynomial `Factorize` implementations cover `Z`, `Q`,
finite fields, and `AlgebraicExtension<Q>` (`poly/factor.rs:4100-4560`), but
not `AlgebraicQuotient<RationalPolynomialField<...>>`. Consequently the bound
on `RationalPolynomial::apart_factored_denominators` cannot be satisfied for
that formal parametric quotient (`domains/rational_polynomial.rs:1240-1348`).

Verdict: **WRAP** Symbolica's native Q-factorization and native
`apart_factored_denominators`; **RETAIN** only HyperFLINT's branch-independent
Wm/Wp naming, remaining-variable guard, and denominator-splitting policy. The
implemented adapter first lets Symbolica decompose over `Q(parameters)`. For
each irreducible quadratic component it allocates registered function
indeterminates, rewrites only that denominator base as
`lc*(x-Wm(i))*(x-Wp(i))`, and invokes the same native Symbolica apart routine
again. No root solver, factorizer, derivative-residue CAS, or external process
is duplicated. `Root<Q(parameters)>` and `AlgebraicQuotient` remain test
oracles for branch formulas/Vieta reduction rather than hot-path storage.

## Pattern replacement, substitution, differentiation, and numerical evaluation

General expression matching is native:

- `Pattern` supports literals, wildcards, functions, powers, products, sums, alternatives, and transformer patterns (`id.rs:45-65`);
- `Replacement`, `ReplaceBuilder`, and `MatchSettings` are public (`id.rs:245-290,424-455,4653-4685`);
- `AtomCore::replace`, `replace_multiple`, `replace_map`, bottom-up map, and `pattern_match` are at `atom/core.rs:1985-2212`;
- `Transformer` provides derivative, series, collect/Horner, replacement, parallel term mapping, repeat, and custom `Map`, with `execute`/`execute_chain` (`transformer.rs:28-41,216-282,532-585`).

Verdict: **REPLACE** general-purpose custom Atom-tree rewrites with these APIs. Keep direct typed polynomial substitution for algebra kernels; a pattern engine is more flexible but is not presumed faster than index-based polynomial replacement.

Expression differentiation is `AtomCore::derivative`/`derivative_into` (`atom/core.rs:663-698`). Hlog/MPL partial derivatives are registered on their fixed heads. `algebra/diff.rs` and `integrator/differentiate.rs` remain typed word algorithms because they return structured terms and avoid general Atom matching; hook tests differential-check against those routines.

Numerical APIs are:

- one-shot fixed-precision `evaluate`, arbitrary-precision `evaluate_with_prec`, and ring-valued `evaluate_in_ring` (`atom/core.rs:930-1007`);
- reusable `evaluator`/`evaluator_multiple` builders (`atom/core.rs:1009-1088`);
- `to_float(decimal_prec)` (`atom/core.rs:1124-1153`);
- heuristic `zero_test(iterations,tolerance)` (`atom/core.rs:1090-1103`).

The reusable evaluator works in the selected build. JIT compilation/export does not because `native_code_generation` is off (`evaluate.rs:7-45`). Use the builder for repeated numerical probes; expected gain comes from compiled instruction reuse, not JIT. `zero_test` explicitly returns `ConditionResult` and can be inconclusive; it must **not** replace exact `reduce/periods.rs::test_zero_function[_sym]` or exact polynomial equality.

## Gröbner API and the pure-Symbolica Euler path

The selected vendored snapshot contains a public in-process F4 implementation:

- `GroebnerBasis<R: Field,E,O>` and public `system` (`poly/groebner.rs:132-150`);
- `GroebnerBasis::new` for `R: Field + Echelonize` (`poly/groebner.rs:729-754`), which runs F4 then reduces the basis;
- polynomial `.reduce(&basis)` (`poly/groebner.rs:1029-1060`), `reduce_basis`, and `is_groebner_basis` (`poly/groebner.rs:1150-1215`);
- FGLM `change_order<O2>` with explicit empty/unified/zero-dimensional checks (`poly/groebner.rs:1269-1320`);
- exact zero-dimensional Q solving to `PolynomialSolution<Q>` and Atom maps (`poly/groebner.rs:138-198,1625-1685`);
- formal parameter-field types `ParameterField`, `ParametricExtension`, `ParametricRoot`, `ParametricSolution` (`poly/groebner.rs:201-244,525-635`) and `solve_parametric` with generic-locus conditions (`poly/groebner.rs:2208-2255`);
- a blanket public `Echelonize` implementation for every `'static` field, with a specialized `Zp` path selected by the F4 kernel (`poly/groebner.rs:2444-2485`).

At expression level, `AtomCore::solve` documents exact linear/polynomial solving over Q and Q(parameters), rational denominator exclusion, rational-power polynomialization, and positive-dimensional partial solutions (`atom/core.rs:826-892`). `SolveBuilder::over`/`wrt` are public (`solve.rs:475-521`); inequalities are accepted by the builder but currently return `InequalitiesNotSupported` (`solve.rs:522-528`).

### Exact mapping of HyperFLINT's Euler chi/filter

HyperFLINT's current C++ contract constructs, for each sector and a finite-field specialization, the cleared logarithmic critical ideal

```text
N_v = sum_i e_i (product_{j != i} P_j) * dP_i/dx_v,
1 - z * product_i P_i,
```

computes its grevlex Gröbner basis modulo a prime, and counts standard monomials from the leading exponents. The upstream contract and system construction are in [`euler_chi.hpp`](https://github.com/SubTropica/SubTropica/blob/main/HyperFLINT/include/hyperflint/algebra/euler_chi.hpp) and [`euler_chi.cpp`](https://github.com/SubTropica/SubTropica/blob/main/HyperFLINT/src/algebra/euler_chi.cpp#L574-L667). The genuineness filter compares generic and constrained counts with conservative repeated draws in [`euler_filter.cpp`](https://github.com/SubTropica/SubTropica/blob/main/HyperFLINT/src/algebra/euler_filter.cpp).

The pure-Symbolica port should map it as follows:

| Euler step | Symbolica API | Verdict |
|---|---|---|
| integer polynomial factors/derivatives/products/evaluation | `MultivariatePolynomial<IntegerRing,u16>` derivative, multiplication, and typed `replace`/`replace_all` | **REPLACE** all FLINT polynomial use. |
| specialize coefficients modulo an odd prime | `Zp::new(p)` (`vendor/symbolica-src/lib/numerica/src/domains/finite_field.rs:34-37,197-230`) and polynomial `map_coeff`; Atom can also convert directly with `to_polynomial(&Zp, ...)` as shown in `poly/groebner.rs:1-43` | **REPLACE** FLINT/nmod conversion. |
| choose a root of the now-univariate constraint | factor the lex-order `MultivariatePolynomial<Zp,...>` through public `Factorize`; select a degree-one factor and compute `-c0/c1` with field operations. Finite-field `Factorize` is implemented in `poly/factor.rs:4560-4610` | **REPLACE** `nmod_poly_roots`, preserving the matched-prime/retry policy. Do not use complex `UnivariatePolynomial::roots`; that API is numerical. |
| grevlex GB | reorder generators to `GrevLexOrder`, then `GroebnerBasis::new(&system,false)` | **REPLACE** the `msolve` subprocess, temp files, serializer, parser, and spawn circuit breaker. |
| leading exponent vectors | `gb.system.iter().map(|p| p.max_exp())`; `max_exp` is public (`poly/polynomial.rs:2075-2087`) | **REPLACE** parsed msolve terms. |
| finite/positive-dimensional staircase and quotient dimension | no public quotient-dimension/standard-monomial API was found. FGLM internally constructs a staircase but does not expose it (`poly/groebner.rs:1293-1505`) | **RETAIN** `chi_staircase_count` as pure combinatorics. Do not use `solve().len()`: it is more expensive and need not represent multiplicity/quotient dimension. |
| sector enumeration, C-star chart, random parameter residues, cache, max-of-two generic count, two independent destructive votes, conservative failure policy | HyperFLINT domain semantics, not a CAS primitive | **RETAIN** exactly and port to typed data. |

This is a credible pure-Symbolica implementation, but not a claim that Symbolica natively implements the entire Euler algorithm. Expected performance gains are elimination of thousands of process launches and text/file serialization. F4 versus `msolve` kernel performance is unknown and must be benchmarked on the same ideals, with process overhead reported separately. Preserve prime limits compatible with `Zp` (`u32`, odd prime), deterministic seeded draws, sector cap, and conservative keep-on-failure behavior.

### Implemented pure-Symbolica Euler path

The migration above is implemented in `src/algebra/euler/{staircase,system,filter}.rs`. The exact public-API anchors used by the implementation are:

- rational-to-primitive-integer coefficient mapping through `MultivariatePolynomial::map_coeff` (`poly/polynomial.rs:2031-2053`) followed by public `make_primitive` (`poly/polynomial.rs:5054-5085`);
- typed specialization through public `replace` (`poly/polynomial.rs:2556-2590`), typed differentiation (`poly/polynomial.rs:2192-2207`), multiplication, and coefficient scaling;
- `Zp::new` and `FiniteFieldCore::{to_element,from_element}` (`lib/numerica/src/domains/finite_field.rs:197-230,290-316`);
- the public finite-field `Factorize` implementation for lex-order multivariate polynomials (`poly/factor.rs:4560-4698`), restricted here to the specialized univariate constraint; all degree-one factors are inspected and the least residue root is selected, so an internal randomized factor ordering cannot affect the chosen root;
- `reorder::<GrevLexOrder>` (`poly/polynomial.rs:1986-2004`), `GroebnerBasis::new` (`poly/groebner.rs:729-754`), and public `max_exp` (`poly/polynomial.rs:2081-2088`).

The remaining code is HyperFLINT domain policy, not a replacement CAS: sector enumeration, cleared-dlog system assembly, homogeneous C-star charting, staircase enumeration, and conservative voting. `SplitMix64` supplies a fully specified caller-seeded specialization stream; the 32 descending primes, matched-prime constraint-root retry, twist seed, stable FNV seed digests, max-of-two generic counts, and two independent destructive votes are deterministic. The per-marginal generic memo uses complete typed `Poly` factors, the exact propagator-index vector, and the derived generic sampling seed as its equality key, so reusing a public cache with another base seed cannot reuse a sampled value or failure. Its manual hash visits the common native context once and then each canonical polynomial payload. The separate digest is seed material only and is never an equality proof: coefficients and exponents are hashed natively, while `AtomCore::to_canonical_string()` supplies a namespace-complete, symbol-registration-order-independent spelling of each native variable. Diagnostic `PolyCtx::vars()` aliases never enter either identity or seed construction. Any invalid input, finite-field factorization/F4 panic, positive-dimensional constrained ideal, or exhausted retry keeps the candidate letter. Process-global atomic stats separately expose system construction and F4 wall time, chi calls, judgments, drops, boundary exemptions, and failure abstentions.

Polynomial factorization likewise keeps Symbolica values typed end to end. The
public rational-field `Factorize::factor` implementation
(`poly/factor.rs:4386-4432`) returns exact polynomial factors and a constant
factor; Hyperbolica folds the latter into a native `Rational`, constructs
constant polynomials with `MultivariatePolynomial::constant`
(`poly/polynomial.rs:956-974`), and carries the target-independent leading
coefficient of a linear factorization as `Poly`. String conversion occurs only
when emitting the legacy JSON protocol.

There is intentionally no `msolve` path, subprocess, temporary-file serializer/parser, spawn breaker, or FLINT-backed fallback in this implementation. Legacy `HF_EULER_FILTER` interpretation is confined to the JSON bridge, which maps it into explicit `LrSearchOptions`/`ScanOptions`; the native typed API itself uses only the explicit option field.

## Native special functions and confirmed domain-specific gaps

Symbolica has the classical one-variable `polylog(s,z)` and Riemann `zeta(s)`. Their public symbols are at `transcendental.rs:2059-2097`, the `TranscendentalFunctions` extension trait at `transcendental.rs:2304-2381`, and normalization/derivative/evaluation hooks at `transcendental.rs:486-648`. Exact simplifications include nonpositive integer orders, `s=1`, `z=0,1,-1`, and zeta reductions (`transcendental.rs:3458-3514`).

Use the native classical polylog for depth-one MPL only after confirming Hyperbolica's branch convention and normalization. It is not an MPL engine.

The following case-insensitive public-source/test/example search returned no matches other than the classical `polylog` material above:

```text
shuffle | hyperlog | multiple polylog | MPL | MZV | multiple zeta |
iterated integral | Hlog | fibration | linear reducibility
```

Search scope was `vendor/symbolica-src/src`, `vendor/symbolica-src/tests`, and `vendor/symbolica-src/examples` in this exact snapshot. Therefore:

- `algebra/shuffle.rs`, word concatenation/collection/regularization: **RETAIN**;
- Hlog/MPL differentiation, conversion, summation, and series: **RETAIN**, while exposing them through fixed Symbolica heads/hooks;
- MZV data loading, expansion/reduction, period conversion, fibration basis, and contour breakup: **RETAIN**;
- LR search/scan, factor-table policies, hyperlog integration, regularization, and transforms: **RETAIN**;
- use native polynomial/RP primitives inside those algorithms rather than reimplementing CAS layers.

## Current module-by-module migration map

This table is deliberately exhaustive at module granularity. “Native primitive” names the Symbolica layer each module must use; **RETAIN** means the domain algorithm remains, not that its current string plumbing should remain.

| Current module(s) | Operation | Native primitive / confirmed gap | Verdict |
|---|---|---|---|
| `algebra/algebraic_letters.rs` | Wm/Wp table, Vieta reduction, back-substitution | fixed function `PolyVariable`s; `Root<Q(parameters)>`; `AlgebraicQuotient` | **WRAP/ORACLE** quotient reduction; retain upstream table/branch contract; structural `(var_idx, Poly)` dedup is implemented. |
| `algebra/convert.rs` | Möbius changes of hyperlog words | typed RP substitution/arithmetic; no hyperlog transform | **RETAIN**, replace string substitutions with typed RP operations. |
| `algebra/diff.rs` | Hlog/MPL structured derivatives | custom derivative hooks exist; no native Hlog/MPL rule | **RETAIN**, with typed rules registered as head callbacks. |
| `algebra/linear_factors.rs` | factor by target variable and extract poles | native `Factorize`, polynomial degree/coefficient; exact roots for parameter-free Q | **WRAP** native factors; retain HyperFLINT result shape/nonlinear remainder. |
| `algebra/partial_fractions.rs` | quotient plus linear poles/multiplicities | native RP `apart_factored_denominators` | **WRAP, P1 complete** directly on native `Rat` storage. |
| `algebra/shuffle.rs` | shuffle, concat, collection, head/tail regularization | no native shuffle/hyperlog API found | **RETAIN**; replace string letter keys with typed hash keys. |
| `api/input.rs`, `api/integrate.rs`, `api/output.rs`, `api/options.rs`, `api/error.rs` | native Atom façade | `Atom`, `AtomView`, indeterminate discovery, `Symbol::call`, `Atom::add_many` | **WRAP**; make concrete `Atom` the public/PyO3 surface, generic `AtomCore` only an internal convenience. |
| `convert/atom.rs` | split Atom into coefficient/Hlog IR | AtomView traversal, patterns | **WRAP** native traversal; retain validated special-function IR conversion. |
| `convert/convert_hlog.rs` | regularized Hlog conversion | no native hyperlog conversion | **RETAIN**. |
| `convert/expr.rs` | narrow expression IR | `Atom` is full expression type | **RETAIN only as private IR** or replace with Atom; do not expose as a second CAS. |
| `convert/parse.rs` | lexer/parser and variable collection | `Atom::parse`, `get_all_indeterminates` | **REPLACE** production parser. |
| `core/poly.rs` | polynomial/context wrapper | native `MultivariatePolynomial<Q,u16>` and all typed methods above | **WRAP, P1 complete**: typed substitution/evaluation/integration and hybrid native rearrangement; conventional discriminant sign fixed; sparse coefficient query retained. |
| `core/rat.rs` | canonical rational arithmetic | native `RationalPolynomial<Z,u16>` | **REPLACE, P1 complete** storage/arithmetic/substitution/evaluation; retain checked signed `pow` and lazy compatibility projections. |
| `core/factored_rat.rs` | factor-preserving arithmetic/derivative/peeling | incomplete native factorized RP | selective **ORACLE/WRAP**; retain robust specialized behavior. |
| `core/canonical_signature.rs` | bucket acceleration | native Atom/poly/RP `Hash` | **WRAP, corrected**: only the crate-private `poly_bucket_digest` remains; digest-only `RatAddKey`/`ReduceKey` and all public digest exports were removed. Every consumer retains full values in collision buckets. |
| `core/context_interner.rs` | weak context interning | Symbolica globally interns variable lists internally (`state.rs:266-273`), but exposes no public weak context-interner contract | **RETAIN, corrected**: ordered `PolyVariable` identity is confirmed inside collision buckets; diagnostic spellings are presentation-only and structural namespaces cannot alias. |
| `core/period_table.rs`, `core/zw_table.rs` | domain-specific handle interning | native structural `Eq + Hash`; no matching handle table | **RETAIN**, change canonical strings to typed keys where possible. |
| `core/rat_split.rs`, `core/sym_coef_split.rs` | split wide/narrow coefficient contexts and ZW handles | polynomial rearrange/map, native RP arithmetic; no equivalent split | **RETAIN**, move storage to native RP and typed context maps. |
| `core/symcoef.rs` | sparse products of pi/i/log/delta/period/MZV factors | Atom can represent all factors; no MPL/MZV canonicalizer | **RETAIN hot representation**, use fixed heads and Atom conversion; benchmark against `Atom::add_many/mul_many` before any replacement. |
| `integrator/differentiate.rs` | wordlist derivative | expression callback exists, no wordlist result API | **RETAIN** and differential-test against native head derivative. |
| `integrator/factor_table.rs` | factor-stage cache/policies | native factor/GCD/resultant; no stage table | **RETAIN** policy; structural collision-safe interning is implemented and strings are emitted only by the bridge. |
| `integrator/primitive.rs` | polynomial/simple-pole hyperlog primitive | native polynomial `.integrate`; RP `.integrate` has different output/caveat | **REPLACE** polynomial loop only; retain word/log construction. |
| `integrator/integration_step.rs`, `hyper_int.rs` | recursive hyperlog integration | no native hyperlog integrator | **RETAIN**; step-local transform reuse now uses collision-safe full `(variable, Vec<Word>)` identity. |
| `integrator/regularize.rs`, `transform.rs` | shuffle regularization/regulators/word transforms | no native hyperlog operation; general `Transformer` is not the same algebra | **RETAIN, P3 complete for transform**; formatted strings remain only for legacy presentation/compatibility ordering. |
| `integrator/lr_search.rs`, `lr_scan.rs` | linear reducibility/Fubini search/conic heuristics | native resultant/discriminant/factor/GCD/Groebner; no LR search | **RETAIN** search and native CAS substeps; collision-safe structural caches/ledgers and the pure-Symbolica Euler backend are implemented. |
| `reduce/break_up_contour.rs` | contour decomposition | no native API found | **RETAIN**, use fixed delta head. |
| `reduce/mzv_reduce.rs`, `mzv_expansion.rs` | table parsing, rational substitution, MZV basis | no native MZV reduction; native patterns/poly substitution available | **RETAIN** data algorithm; replace encoded symbol names with `MZV(...)`; retain typed Horner rational substitution if benchmarks beat Atom replacement. |
| `reduce/periods.rs` | period-to-MZV, fibration, exact zero test | no native MPL/MZV/fibration; native `zero_test` is heuristic | **RETAIN**; fibration accumulation uses collision-safe full `RegKey` identity. Never replace exact test with `zero_test`. |
| `series/laurent.rs` | exact rational Laurent recurrence/reciprocal/constant | native `AtomCore::series`/`Series<AtomField>` | **RETAIN + ORACLE**, benchmark. |
| `series/expansions.rs`, `hlog_series.rs`, `mpl_series.rs`, `mpl_sum.rs` | Hlog/MPL series and finite sums | no native MPL/Hlog API | **RETAIN**, exposed through conservative head callbacks in valid regimes. |
| `symbols.rs`, `symbols/hooks/**`, `symbols/hlog.rs`, `symbols/mpl.rs`, `symbols/word.rs` | fixed heads, expression hooks, and typed domain objects | `initialize!`, derivative/series callbacks, function `PolyVariable`s | **WRAP, P2 complete**; retain typed word/Hlog/MPL objects and structural identities. |
| `bridge/**`, `c_abi.rs`, `bin/hyperflint.rs` | JSON/C/CLI compatibility transport | Symbolica parsing/printing only at boundary | **RETAIN transport**, but route all algebra through Atom/typed core. Do not let wire strings become cache/CAS state. |
| `python/**` | PyO3 Atom API, prepared calls, result/exceptions/options | Symbolica is built with `python_export` under root `python`; native Atom types can cross the façade | **WRAP** the Rust Atom API. Keep Hyperbolica-specific Python classes/options/errors; no independent parsing/CAS. |
| `lib.rs`, `error.rs` | module/API exports and errors | no CAS duplication | **RETAIN**, expose native-Atom-first API and wrap Symbolica failures without panics. |

## Hand-written native-CAS duplication: prioritized deletion list

| Priority | Current implementation | Native replacement | Expected impact / constraint |
|---|---|---|---|
| P0 | `Poly::discriminant` signed quotient | native derivative/resultant/lcoeff plus conventional sign | Correctness fix; negligible cost. |
| P1 | `Rat::new`, add, mul, div, derivative | integer `RationalPolynomial` construction/operators/derivative | Expected major simplification and PF speedup; normalization invariant changes. Keep fast signed pow. |
| P1 | PF Q→Z→Q adapters | direct native `Rat` + `apart_factored_denominators` | Expected fewer allocations/conversions; retain output checks. |
| P1 | `Poly` string substitute/evaluate and bridge reparsing | typed polynomial `replace`/evaluation | **Implemented.** Production APIs take `Rational`/`Integer`; string conversion exists only in the JSON wire adapter. Criterion cases cover sparse substitution/evaluation. |
| P1 | manual primitive polynomial loop | polynomial `.integrate` | **Implemented.** Exact field division is native; derivative round-trip tests include sparse and parameter-denominator cases. |
| P1 | Euler external GB/process parser | `Zp` + native F4 `GroebnerBasis` | Expected very large orchestration win; kernel comparison unknown; staircase/domain policy remains. |
| P2 | custom parser as public algebra parser | `Atom::parse` + Atom traversal | Removes duplicate grammar and string variable scans; preserve explicit legacy compatibility if required. |
| P2 | general Atom tree rewrite/scanning | `Pattern`, `Replacement`, `replace_map`, `Transformer` | Correctness/maintainability gain; benchmark hot rewrites before using patterns inside polynomial loops. |
| P2 | manual general-expression derivative | `AtomCore::derivative` plus registered callbacks | Consistent chain rule; typed structured derivative remains. |
| P2 | exact Laurent recurrence | `AtomCore::series` | Do not delete yet: broader native functionality versus likely Atom overhead. Differential oracle first. |
| P3 | string canonical signatures/content keys | native structural hashes | **Implemented for LR search/scan/factor-table, primitive, transform, period fibration, and integration-step caches.** Full typed values remain in collision buckets; `u64` digests are accelerators only. Remaining domain tables are separately tracked. |

## Correctness and performance gates

No migration is complete merely because it compiles. Each replacement must satisfy these gates.

1. **Value equivalence:** compare canonical Atoms after `cancel()` or compare native polynomial/RP values. Do not compare old/new numerator-denominator shapes when their normalization invariants differ.
2. **Rat property tests:** for random sparse Q polynomials, cover construction, add/sub/mul/div, signed powers, derivative, substitutions, evaluation, and zero/division errors. Check `to_atom().cancel()` against the old implementation during migration.
3. **Partial fractions:** reconstruct every output exactly; test repeated poles, nonmonic linear bases, parameter coefficients, polynomial parts, irreducible nonlinear factors, zero, and variable-independent denominators.
4. **Resultant/discriminant:** cross-check `resultant`, `resultant_brown`, and `resultant_crt` on bounded random inputs. Verify the discriminant against known degrees 1-5 and its scaling law.
5. **Series:** multiply the truncated result by the original denominator and verify coefficients through the requested order. Differential-test native Atom series and the rational recurrence at zero/nonzero points, poles, sparse gaps, and rational powers.
6. **Algebraic letters:** compare native quotient/root reductions to every current Vieta/back-substitution fixture. Test branch/index behavior only after parameter specialization where required.
7. **Euler:** compare leading ideals and staircase counts with upstream fixtures for each sector, prime, and specialization; separately compare final conservative letter verdicts. Test positive dimension, unit ideal, vanishing factors, constrained rootless primes, homogeneous charting, deterministic seeds, and failure fallback.
8. **Hooks:** compare native `Atom::derivative` and `.series` of `Hlog`/`Mpl` calls with typed algorithms, including nested arguments to catch double/missing chain-rule factors.
9. **No-FLINT:** run both dependency-tree guards under default and Python feature sets.

Benchmark in release/LTO mode and report medians plus allocation/peak-memory data where available. The minimum corpus should include tiny algebra (to catch wrapper overhead), sparse high-degree polynomials, dense parameter-heavy resultants, repeated shared-denominator Rat arithmetic, large PF multiplicities, LR/factor-table fixtures, high-order Laurent series, MZV table reductions, and Euler ideals. For Euler, report four numbers independently: system construction, finite-field conversion, F4 kernel, and old process/serialization overhead. “On par” means no statistically meaningful regression on the geometric mean and no unexplained severe tail regression; any retained specialized path must have a corpus showing why.

## Re-audit rule

Before implementing any additional CAS algorithm, search all of:

1. public re-exports in `vendor/symbolica-src/src/lib.rs` and `prelude`;
2. the relevant `atom/core.rs`, `poly/**`, `domains/**`, `solve.rs`, `transformer.rs`, and `transcendental.rs` implementation;
3. in-tree tests/examples for behavior and unsupported cases;
4. trait bounds and feature gates at the method's actual `impl`, not only its name.

Record a source anchor and a retain/replace/wrap verdict in this document before adding the implementation. Re-run the negative domain-capability search when the vendored Symbolica revision changes.
