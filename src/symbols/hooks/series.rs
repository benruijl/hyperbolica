//! Exact finite-precision series transforms for canonical special functions.

use std::collections::BTreeSet;

use symbolica::domains::atom::AtomField;
use symbolica::poly::PolyVariable;
use symbolica::prelude::{Atom, AtomCore, Integer, Series};

use crate::core::{PolyCtx, Rat};
use crate::series::{hlog_zero_expand, mpl_sum};
use crate::symbols::Word;

const MAX_HOOK_ORDER: i64 = 512;

/// Expand `Hlog(z,a1,...)` only in the exact zero-endpoint regime covered by
/// Hyperbolica's typed Panzer/Brown recurrence. Vanishing nonzero letters and
/// argument poles require a different asymptotic analysis and are deferred.
pub(in crate::symbols) fn series_hlog(arguments: &[Series<AtomField>]) -> Option<(Atom, Atom)> {
    let (endpoint, letters) = arguments.split_first()?;
    if letters.is_empty() || endpoint.is_zero() {
        return None;
    }
    let endpoint_order = endpoint.get_trailing_exponent();
    if endpoint_order <= 0 {
        return None;
    }
    if letters.iter().any(|letter| {
        let trailing = letter.get_trailing_exponent();
        trailing.is_negative() || (trailing > 0 && !exact_zero_series(letter))
    }) {
        return None;
    }

    let target_order = arguments.iter().map(Series::absolute_order).min()?;
    let expansion_order = (&target_order / &endpoint_order).ceil().to_i64()?;
    if !(0..=MAX_HOOK_ORDER).contains(&expansion_order) {
        return None;
    }

    let typed = rationalize(arguments)?;
    let endpoint = &typed.values[0];
    let word = Word::new(typed.values[1..].to_vec());
    let terms = hlog_zero_expand(endpoint, &word, expansion_order).ok()?;
    let endpoint_atom = &typed.atoms[0];
    let mut summands = Vec::with_capacity(terms.len());
    for term in terms {
        let mut factors = vec![term.coef.to_atom()];
        if term.log_power > 0 {
            let log_power = u32::try_from(term.log_power).ok()?;
            factors.push(
                endpoint_atom.log().pow(term.log_power) / Atom::num(Integer::factorial(log_power)),
            );
        }
        if term.arg_power > 0 {
            factors.push(endpoint_atom.pow(term.arg_power));
        }
        summands.push(Atom::mul_many(factors));
    }
    let regularized = Atom::add_many(summands);
    Some((Atom::one(), regularized))
}

/// Transform canonical `Mpl(n1,...,nd,z1,...,zd)` calls.
///
/// Depth one is exactly Symbolica's principal-branch classical polylogarithm.
/// At any depth, a regular last argument tending to zero admits the finite
/// defining sum. The tempting first-argument-only zero shortcut is not exact
/// at fixed expansion order for depth greater than one, so it is not used.
pub(in crate::symbols) fn series_mpl(arguments: &[Series<AtomField>]) -> Option<(Atom, Atom)> {
    if arguments.is_empty() || !arguments.len().is_multiple_of(2) {
        return None;
    }
    let depth = arguments.len() / 2;
    let indices = arguments[..depth]
        .iter()
        .map(series_positive_integer)
        .collect::<Option<Vec<_>>>()?;
    let value_series = &arguments[depth..];
    let value_atoms = value_series.iter().map(Series::to_atom).collect::<Vec<_>>();

    let last = value_series.last()?;
    if exact_zero_series(last) {
        return Some((Atom::one(), Atom::zero()));
    }
    let last_order = last.get_trailing_exponent();
    if last_order > 0
        && value_series.iter().all(|value| {
            !value.get_trailing_exponent().is_negative()
                && (!value.is_zero() || exact_zero_series(value))
        })
    {
        let target_order = value_series.iter().map(Series::absolute_order).min()?;
        let summation_order = (&target_order / &last_order).ceil().to_i64()?;
        if (0..=MAX_HOOK_ORDER).contains(&summation_order) {
            let typed = rationalize(value_series)?;
            let scalar = mpl_sum(&indices, &typed.values, summation_order).ok()?;
            return Some((Atom::one(), scalar.to_atom()));
        }
        return None;
    }

    if depth == 1 {
        let polylog = symbolica::transcendental::polylog()
            .call((Atom::num(indices[0]), value_atoms[0].clone()));
        return Some((Atom::one(), polylog));
    }

    None
}

fn exact_zero_series(series: &Series<AtomField>) -> bool {
    series.is_zero() && series.is_constant()
}

fn series_positive_integer(series: &Series<AtomField>) -> Option<i64> {
    if !series.is_constant() {
        return None;
    }
    super::shape::positive_integer(series.coefficient(0.into()).as_view())
}

struct RationalizedSeries {
    atoms: Vec<Atom>,
    values: Vec<Rat>,
}

fn rationalize(series: &[Series<AtomField>]) -> Option<RationalizedSeries> {
    let first = series.first()?;
    let variable = match first.get_variable().as_ref() {
        PolyVariable::Temporary(_) => return None,
        variable => variable.to_atom(),
    };
    if series
        .iter()
        .skip(1)
        .any(|value| value.get_variable() != first.get_variable())
    {
        return None;
    }

    let atoms = series.iter().map(Series::to_atom).collect::<Vec<_>>();
    let mut indeterminates = BTreeSet::new();
    for atom in &atoms {
        indeterminates.extend(
            atom.get_all_indeterminates(false)
                .into_iter()
                .map(|view| view.to_owned()),
        );
    }
    indeterminates.remove(&variable);
    let variables = std::iter::once(variable.clone())
        .chain(indeterminates)
        .collect::<Vec<_>>();
    let context = PolyCtx::from_indeterminates(variables).ok()?;
    let variable_index = context.index_of_indeterminate(variable.as_view())?;
    if variable_index != 0 {
        return None;
    }
    let values = atoms
        .iter()
        .map(|atom| Rat::from_atom(context.clone(), atom.as_view()).ok())
        .collect::<Option<Vec<_>>>()?;
    Some(RationalizedSeries { atoms, values })
}

#[cfg(test)]
mod tests {
    use symbolica::poly::series::{SeriesDepth, SeriesError};
    use symbolica::prelude::{AtomCore, Rational, Symbol, symbol};

    use super::*;
    use crate::core::PolyCtx;
    use crate::symbols::heads;

    fn hlog_expansion_atom(endpoint: &Rat, word: &Word, order: i64) -> Atom {
        let terms = hlog_zero_expand(endpoint, word, order).unwrap();
        Atom::add_many(terms.into_iter().map(|term| {
            let mut factors = vec![term.coef.to_atom()];
            if term.log_power > 0 {
                factors.push(
                    endpoint.to_atom().log().pow(term.log_power)
                        / Atom::num(Integer::factorial(term.log_power as u32)),
                );
            }
            if term.arg_power > 0 {
                factors.push(endpoint.to_atom().pow(term.arg_power));
            }
            Atom::mul_many(factors)
        }))
    }

    fn expand_at_zero(input: &Atom, variable: Symbol, order: i64) -> Series<AtomField> {
        input
            .series(
                variable,
                Atom::zero().as_view(),
                SeriesDepth::absolute(order),
            )
            .unwrap()
    }

    #[test]
    fn hlog_zero_series_matches_typed_recurrence_at_depths_one_two_and_three() {
        let x = symbol!("hook_hlog_series_x");
        let ctx = PolyCtx::from_symbols([x]).unwrap();
        let endpoint = Rat::from_atom(ctx.clone(), x.to_atom().as_view()).unwrap();
        let words = [
            Word::new(vec![Rat::from_int(ctx.clone(), 0)]),
            Word::new(vec![Rat::from_int(ctx.clone(), 1)]),
            Word::new(vec![
                Rat::from_int(ctx.clone(), 0),
                Rat::from_int(ctx.clone(), 1),
            ]),
            Word::new(vec![
                Rat::from_int(ctx.clone(), 0),
                Rat::from_int(ctx.clone(), 1),
                Rat::from_int(ctx.clone(), -1),
            ]),
        ];

        for word in words {
            let input = heads().hlog.call_args(
                std::iter::once(endpoint.to_atom()).chain(word.letters.iter().map(Rat::to_atom)),
            );
            let actual = expand_at_zero(&input, x, 3).to_atom().expand();
            let expected = hlog_expansion_atom(&endpoint, &word, 3).expand();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn regular_hlog_series_uses_partial_derivatives_without_a_chain_rule_duplicate() {
        let x = symbol!("hook_hlog_regular_x");
        let endpoint = Atom::one() + x;
        let constant = heads().hlog.call((1, 2));
        let input = heads().hlog.call((endpoint, 2));
        let actual = expand_at_zero(&input, x, 3).to_atom();
        let expected = constant - x - x.to_atom().pow(2) / 2 - x.to_atom().pow(3) / 3;
        assert_eq!(actual.expand(), expected.expand());
    }

    #[test]
    fn mpl_zero_series_matches_typed_defining_sum_at_depth_one_and_two() {
        let (x, a) = symbol!("hook_mpl_series_x", "hook_mpl_series_a");
        let ctx = PolyCtx::from_symbols([x, a]).unwrap();
        let x_rat = Rat::from_atom(ctx.clone(), x.to_atom().as_view()).unwrap();
        let a_rat = Rat::from_atom(ctx.clone(), a.to_atom().as_view()).unwrap();
        let cases = [
            (vec![2], vec![x_rat.clone()]),
            (vec![1, 2], vec![a_rat, x_rat]),
        ];

        for (indices, arguments) in cases {
            let input = heads().mpl.call_args(
                indices
                    .iter()
                    .copied()
                    .map(Atom::num)
                    .chain(arguments.iter().map(Rat::to_atom)),
            );
            let actual = expand_at_zero(&input, x, 3).to_atom().expand();
            let expected = mpl_sum(&indices, &arguments, 3).unwrap().to_atom().expand();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn regular_depth_one_mpl_delegates_exactly_to_native_polylog() {
        let x = symbol!("hook_mpl_polylog_x");
        let point = Atom::num(Rational::from((1, 2)));
        let argument = point + x;
        let mpl = heads().mpl.call((2, argument.clone()));
        let native = symbolica::transcendental::polylog().call((2, argument));
        assert_eq!(
            expand_at_zero(&mpl, x, 3).to_atom(),
            expand_at_zero(&native, x, 3).to_atom()
        );
    }

    #[test]
    fn weight_one_mpl_exposes_its_logarithmic_singularity() {
        let x = symbol!("hook_mpl_log_x");
        let input = heads().mpl.call((1, Atom::one() - x));
        assert_eq!(expand_at_zero(&input, x, 3).to_atom(), -Symbol::LOG.call(x));
    }

    #[test]
    fn unsupported_argument_poles_and_first_only_mpl_zero_defer_safely() {
        let x = symbol!("hook_series_defer_x");
        let hlog = heads().hlog.call((Atom::one() / x, Atom::one()));
        assert!(matches!(
            hlog.series(x, Atom::zero().as_view(), SeriesDepth::absolute(3)),
            Err(SeriesError::FunctionArgumentPole { .. })
        ));

        let callback = heads().mpl.get_series_function().unwrap();
        let callback_arguments = [Atom::one(), Atom::one(), x.to_atom(), Atom::one()]
            .iter()
            .map(|argument| expand_at_zero(argument, x, 3))
            .collect::<Vec<_>>();
        assert!(callback(&callback_arguments).is_none());
    }

    #[test]
    fn malformed_mpl_series_arity_is_not_reinterpreted() {
        let x = symbol!("hook_series_malformed_x");
        let callback = heads().mpl.get_series_function().unwrap();
        let callback_arguments = [Atom::one(), x.to_atom(), Atom::one()]
            .iter()
            .map(|argument| expand_at_zero(argument, x, 2))
            .collect::<Vec<_>>();
        assert!(callback(&callback_arguments).is_none());
    }
}
