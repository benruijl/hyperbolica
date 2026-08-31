//! Exact partial derivatives for canonical `Hlog` and `Mpl` calls.
//!
//! Symbolica multiplies each value produced here by the derivative of the
//! corresponding function argument. These functions therefore return partial
//! derivatives only; adding a chain-rule factor here would double-count it.

use symbolica::prelude::{Atom, AtomCore, AtomView};
use symbolica::utils::Settable;

use super::shape::{hlog_atom, mpl_atom, split_mpl};

pub(in crate::symbols) fn differentiate_hlog(
    input: AtomView<'_>,
    argument_index: usize,
    output: &mut Settable<'_, Atom>,
) {
    let Some(function) = input.as_fun_view() else {
        return;
    };
    let arguments = function.iter().collect::<Vec<_>>();
    let Some((&endpoint, word)) = arguments.split_first() else {
        return;
    };
    if argument_index >= arguments.len() {
        return;
    }
    if word.is_empty() {
        output.to_num(0);
        return;
    }
    // The defining iterated integral is singular when its endpoint coincides
    // with the first letter.  Even a partial with respect to a later letter
    // is not assigned a rational formula at that singular point.
    if (endpoint.to_owned() - word[0]).is_zero() {
        return;
    }

    let derivative = if argument_index == 0 {
        endpoint_partial(function.get_symbol(), endpoint, word)
    } else {
        letter_partial(function.get_symbol(), endpoint, word, argument_index - 1)
    };
    if let Some(derivative) = derivative {
        **output = derivative;
    }
}

fn endpoint_partial(
    symbol: symbolica::prelude::Symbol,
    endpoint: AtomView<'_>,
    word: &[AtomView<'_>],
) -> Option<Atom> {
    let denominator = endpoint.to_owned() - word[0];
    if denominator.is_zero() {
        return None;
    }
    Some(hlog_atom(symbol, endpoint, &word[1..]) / denominator)
}

fn letter_partial(
    symbol: symbolica::prelude::Symbol,
    endpoint: AtomView<'_>,
    word: &[AtomView<'_>],
    letter_index: usize,
) -> Option<Atom> {
    let mut terms = Vec::with_capacity(6);

    if letter_index > 0 {
        let denominator = word[letter_index - 1].to_owned() - word[letter_index];
        if denominator.is_zero() {
            return None;
        }
        let mut without_current = word.to_vec();
        without_current.remove(letter_index);
        let mut without_previous = word.to_vec();
        without_previous.remove(letter_index - 1);
        terms.push(-hlog_atom(symbol, endpoint, &without_current) / &denominator);
        terms.push(hlog_atom(symbol, endpoint, &without_previous) / denominator);
    }

    if letter_index + 1 < word.len() {
        let denominator = word[letter_index].to_owned() - word[letter_index + 1];
        if denominator.is_zero() {
            return None;
        }
        let mut without_next = word.to_vec();
        without_next.remove(letter_index + 1);
        let mut without_current = word.to_vec();
        without_current.remove(letter_index);
        terms.push(hlog_atom(symbol, endpoint, &without_next) / &denominator);
        terms.push(-hlog_atom(symbol, endpoint, &without_current) / denominator);
    }

    if letter_index + 1 == word.len() {
        // The formal partial with respect to the final letter contains
        // `-Hlog(z,prefix)/a_last`.  A literal zero letter is harmless when
        // it is constant (the derivative engine then never requests this
        // partial), but the partial itself is singular and must stay generic.
        if word[letter_index].is_zero() {
            return None;
        }
        let mut prefix = word.to_vec();
        prefix.pop();
        terms.push(-hlog_atom(symbol, endpoint, &prefix) / word[letter_index]);
    }

    if letter_index == 0 {
        let denominator = endpoint.to_owned() - word[0];
        if denominator.is_zero() {
            return None;
        }
        terms.push(-hlog_atom(symbol, endpoint, &word[1..]) / denominator);
    }

    Some(Atom::add_many(terms))
}

pub(in crate::symbols) fn differentiate_mpl(
    input: AtomView<'_>,
    argument_index: usize,
    output: &mut Settable<'_, Atom>,
) {
    let Some(function) = input.as_fun_view() else {
        return;
    };
    let Some((indices, argument_views)) = split_mpl(function) else {
        return;
    };
    let depth = indices.len();
    if argument_index < depth || argument_index >= depth * 2 {
        // The positive integer orders are discrete parameters, not analytic
        // variables. Preserve a generic derivative if one is ever requested.
        return;
    }
    let arguments = argument_views
        .iter()
        .map(AtomView::to_owned)
        .collect::<Vec<_>>();
    let position = argument_index - depth;
    if let Some(derivative) =
        mpl_argument_partial(function.get_symbol(), &indices, &arguments, position)
    {
        **output = derivative;
    }
}

fn mpl_argument_partial(
    symbol: symbolica::prelude::Symbol,
    indices: &[i64],
    arguments: &[Atom],
    position: usize,
) -> Option<Atom> {
    let argument = &arguments[position];
    if indices[position] > 1 {
        if argument.is_zero() {
            return None;
        }
        let mut lowered = indices.to_vec();
        lowered[position] -= 1;
        return Some(mpl_atom(symbol, &lowered, arguments) / argument);
    }

    if indices.len() == 1 {
        if argument.is_one() {
            return None;
        }
        return Some(Atom::one() / (Atom::one() - argument));
    }

    if position + 1 == indices.len() {
        if argument.is_one() {
            return None;
        }
        let mut shortened_arguments = arguments[..arguments.len() - 1].to_vec();
        let last = shortened_arguments.len() - 1;
        shortened_arguments[last] = &arguments[last] * argument;
        return Some(
            mpl_atom(symbol, &indices[..indices.len() - 1], &shortened_arguments)
                / (Atom::one() - argument),
        );
    }

    if argument.is_zero() || argument.is_one() {
        return None;
    }
    let shortened_indices = indices
        .iter()
        .enumerate()
        .filter_map(|(index, value)| (index != position).then_some(*value))
        .collect::<Vec<_>>();
    let shortened_arguments = arguments
        .iter()
        .enumerate()
        .filter_map(|(index, value)| (index != position).then_some(value.clone()))
        .collect::<Vec<_>>();
    let denominator = argument * (argument - 1);
    let second_denominator = argument - 1;

    if position == 0 {
        let mut merged_right = shortened_arguments.clone();
        merged_right[0] = argument * &arguments[1];
        return Some(Atom::add_many([
            mpl_atom(symbol, &shortened_indices, &merged_right) / denominator,
            -mpl_atom(symbol, &shortened_indices, &shortened_arguments) / second_denominator,
        ]));
    }

    let mut merged_right = shortened_arguments.clone();
    merged_right[position] = argument * &arguments[position + 1];
    let mut merged_left = shortened_arguments;
    merged_left[position - 1] = &arguments[position - 1] * argument;
    Some(Atom::add_many([
        mpl_atom(symbol, &shortened_indices, &merged_right) / denominator,
        -mpl_atom(symbol, &shortened_indices, &merged_left) / second_denominator,
    ]))
}

#[cfg(test)]
mod tests {
    use symbolica::prelude::{Atom, AtomCore, Symbol, symbol};
    use symbolica::utils::Settable;

    use crate::algebra::diff::{diff_hlog, diff_mpl};
    use crate::core::{PolyCtx, Rat};
    use crate::symbols::{Hlog, Mpl, Word, heads};

    fn hlog_to_atom(value: &Hlog) -> Atom {
        if value.word.is_empty() {
            Atom::one()
        } else {
            heads().hlog.call_args(
                std::iter::once(value.z.to_atom())
                    .chain(value.word.letters.iter().map(Rat::to_atom)),
            )
        }
    }

    fn mpl_to_atom(value: &Mpl) -> Atom {
        if value.indices.is_empty() {
            Atom::one()
        } else {
            heads().mpl.call_args(
                value
                    .indices
                    .iter()
                    .copied()
                    .map(Atom::num)
                    .chain(value.args.iter().map(Rat::to_atom)),
            )
        }
    }

    fn typed_hlog_derivative(endpoint: &Rat, word: &Word, variable: usize) -> Atom {
        Atom::add_many(
            diff_hlog(endpoint, word, variable)
                .unwrap()
                .into_iter()
                .map(|term| term.coef.to_atom() * hlog_to_atom(&term.hlog)),
        )
    }

    fn typed_mpl_derivative(indices: &[i64], arguments: &[Rat], variable: usize) -> Atom {
        Atom::add_many(
            diff_mpl(indices, arguments, variable)
                .unwrap()
                .into_iter()
                .map(|term| term.coef.to_atom() * mpl_to_atom(&term.mpl)),
        )
    }

    #[test]
    fn both_registered_heads_expose_native_callbacks() {
        let symbols = heads();
        for symbol in [symbols.hlog, symbols.mpl] {
            assert!(symbol.get_derivative_function().is_some());
            assert!(symbol.get_series_function().is_some());
            assert!(symbol.get_normalization_function().is_some());
        }
    }

    #[test]
    fn empty_hlog_and_mpl_are_structural_units() {
        let x = symbol!("hook_empty_x");
        assert!(heads().hlog.call(x).is_one());
        assert!(heads().mpl.call_args(std::iter::empty::<Atom>()).is_one());
    }

    #[test]
    fn hlog_chain_rule_matches_typed_depth_one_and_depth_three() {
        let (x, y) = symbol!("hook_hlog_x", "hook_hlog_y");
        let ctx = PolyCtx::from_symbols([x, y]).unwrap();
        let cases = [
            (
                Rat::from_atom(ctx.clone(), x.to_atom().pow(2).as_view()).unwrap(),
                Word::new(vec![Rat::from_int(ctx.clone(), 0)]),
            ),
            (
                Rat::from_atom(ctx.clone(), (x.to_atom().pow(2) + y).as_view()).unwrap(),
                Word::new(vec![
                    Rat::from_atom(ctx.clone(), (x + 1).as_view()).unwrap(),
                    Rat::from_atom(ctx.clone(), (x * y + 2).as_view()).unwrap(),
                    Rat::from_atom(ctx.clone(), (y + 3).as_view()).unwrap(),
                ]),
            ),
        ];

        for (endpoint, word) in cases {
            let input = heads().hlog.call_args(
                std::iter::once(endpoint.to_atom()).chain(word.letters.iter().map(Rat::to_atom)),
            );
            let actual = input.derivative(x).cancel();
            let expected = typed_hlog_derivative(&endpoint, &word, 0).cancel();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn nested_endpoint_derivative_is_applied_exactly_once() {
        let x = symbol!("hook_hlog_nested_x");
        let input = heads().hlog.call((x.to_atom().pow(2), Atom::zero()));
        assert_eq!(input.derivative(x).cancel(), (Atom::num(2) / x).cancel());
    }

    #[test]
    fn mpl_chain_rule_matches_typed_depth_one_and_depth_three() {
        let (x, y) = symbol!("hook_mpl_x", "hook_mpl_y");
        let ctx = PolyCtx::from_symbols([x, y]).unwrap();
        let cases = [
            (
                vec![2],
                vec![Rat::from_atom(ctx.clone(), x.to_atom().pow(2).as_view()).unwrap()],
            ),
            (
                vec![2, 1, 1],
                vec![
                    Rat::from_atom(ctx.clone(), x.to_atom().pow(2).as_view()).unwrap(),
                    Rat::from_atom(ctx.clone(), (x * y + 2).as_view()).unwrap(),
                    Rat::from_atom(ctx.clone(), (y + 3).as_view()).unwrap(),
                ],
            ),
        ];

        for (indices, arguments) in cases {
            let input = heads().mpl.call_args(
                indices
                    .iter()
                    .copied()
                    .map(Atom::num)
                    .chain(arguments.iter().map(Rat::to_atom)),
            );
            let actual = input.derivative(x).cancel();
            let expected = typed_mpl_derivative(&indices, &arguments, 0).cancel();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn weight_one_mpl_reduces_to_its_exact_rational_derivative() {
        let x = symbol!("hook_mpl_weight_one_x");
        let input = heads().mpl.call((1, x));
        assert_eq!(
            input.derivative(x).cancel(),
            (Atom::one() / (Atom::one() - x)).cancel()
        );
    }

    #[test]
    fn arg_lists_normalize_to_the_even_flattened_mpl_shape() {
        let (x, y) = symbol!("hook_mpl_arg_x", "hook_mpl_arg_y");
        let indices = Symbol::ARG.call((1, 2));
        let arguments = Symbol::ARG.call((x, y));
        let input = heads().mpl.call((indices, arguments));
        let function = input.as_fun_view().unwrap();
        assert_eq!(function.get_nargs(), 4);
        assert_eq!(function.get(0), Atom::num(1).as_view());
        assert_eq!(function.get(2), x.to_atom().as_view());
    }

    #[test]
    fn malformed_or_singular_shapes_defer_to_symbolicas_generic_derivative() {
        let (x, y) = symbol!("hook_defer_x", "hook_defer_y");
        for input in [
            heads().mpl.call((1, x, y)),
            heads().mpl.call((0, x)),
            heads().hlog.call((x, x)),
        ] {
            let variable = if input.contains(y) { y } else { x };
            assert!(
                input
                    .derivative(variable)
                    .contains_symbol(Symbol::DERIVATIVE)
            );
        }
    }

    #[test]
    fn singular_hlog_partials_leave_the_callback_unset() {
        let (x, y) = symbol!("hook_partial_defer_x", "hook_partial_defer_y");
        let callback = heads().hlog.get_derivative_function().unwrap();

        for (input, argument_index) in [
            // The whole iterated integral is singular at z == a1, including
            // a partial with respect to a later, otherwise regular letter.
            (heads().hlog.call((x, x, y)), 2),
            // The partial with respect to a literal final zero letter has an
            // unavoidable 1/a_last factor.  (Normal differentiation never
            // requests it because the literal zero has zero derivative.)
            (heads().hlog.call((x, y, Atom::zero())), 2),
        ] {
            let mut value = Atom::zero();
            let mut output = Settable::from(&mut value);
            callback(input.as_view(), argument_index, &mut output);
            assert!(!output.is_set());
        }
    }
}
