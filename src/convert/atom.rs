//! Symbolica-`Atom` input adapter for the hyperlogarithm conversion core.

use std::collections::HashSet;
use std::sync::Arc;

use symbolica::atom::representation::FunView;
use symbolica::prelude::{Atom, AtomCore, AtomView, ConvertToRing, Q, Symbol};

use crate::convert::Expr;
use crate::core::{PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::symbols::{Word, heads};

/// An Atom expression lowered into Hyperbolica's typed conversion tree.
#[derive(Clone, Debug)]
pub struct AtomExpression {
    pub expr: Expr,
    pub ctx: Arc<PolyCtx>,
    pub indeterminates: Vec<Atom>,
}

fn collect_rational_indeterminates(atom: AtomView<'_>, output: &mut HashSet<Atom>) {
    let hlog = heads().hlog;
    match atom {
        AtomView::Fun(function) if function.get_symbol() == hlog => {
            for argument in function {
                collect_hlog_argument_indeterminates(argument, output);
            }
        }
        AtomView::Add(addition) => {
            for child in addition {
                collect_rational_indeterminates(child, output);
            }
        }
        AtomView::Mul(product) => {
            for child in product {
                collect_rational_indeterminates(child, output);
            }
        }
        AtomView::Pow(power) => {
            collect_rational_indeterminates(power.get_base(), output);
        }
        _ => {
            for variable in atom.get_all_indeterminates(false) {
                output.insert(variable.to_owned());
            }
        }
    }
}

fn collect_hlog_argument_indeterminates(argument: AtomView<'_>, output: &mut HashSet<Atom>) {
    if let AtomView::Fun(list) = argument
        && list.get_symbol() == Symbol::ARG
    {
        for item in list {
            for variable in item.get_all_indeterminates(false) {
                output.insert(variable.to_owned());
            }
        }
    } else {
        for variable in argument.get_all_indeterminates(false) {
            output.insert(variable.to_owned());
        }
    }
}

fn ordered_indeterminates(
    atom: AtomView<'_>,
    integration_variables: &[Symbol],
    additional_indeterminates: &[Atom],
) -> Vec<Atom> {
    let mut discovered = HashSet::new();
    collect_rational_indeterminates(atom, &mut discovered);

    let mut ordered = Vec::new();
    for variable in integration_variables {
        let atom = variable.to_atom();
        if !ordered.iter().any(|candidate| candidate == &atom) {
            discovered.remove(&atom);
            ordered.push(atom);
        }
    }
    let mut remaining = discovered.into_iter().collect::<Vec<_>>();
    remaining.sort();
    ordered.extend(remaining);
    for atom in additional_indeterminates {
        if !ordered.iter().any(|candidate| candidate == atom) {
            ordered.push(atom.clone());
        }
    }
    ordered
}

fn positive_integer(atom: AtomView<'_>) -> Result<i64> {
    let AtomView::Num(number) = atom else {
        return Err(Error::InvalidInput(
            "a hyperlogarithm expression power must be an integer".into(),
        ));
    };
    let rational = Q
        .try_element_from_coefficient_view(number.get_coeff_view())
        .map_err(Error::InvalidInput)?;
    if !rational.is_integer() {
        return Err(Error::InvalidInput(
            "a hyperlogarithm expression power must be an integer".into(),
        ));
    }
    let exponent = rational.numerator_ref().to_i64().ok_or_else(|| {
        Error::InvalidInput("hyperlogarithm expression exponent does not fit in i64".into())
    })?;
    if exponent < 1 {
        return Err(Error::InvalidExponent(exponent));
    }
    Ok(exponent)
}

fn lower_hlog(function: FunView<'_>, ctx: &Arc<PolyCtx>) -> Result<Expr> {
    if function.get_nargs() == 0 {
        return Err(Error::InvalidInput(
            "Hlog requires an argument followed by zero or more letters".into(),
        ));
    }
    let argument = Rat::from_atom(ctx.clone(), function.get(0))?;
    let mut letter_atoms = function.iter().skip(1).collect::<Vec<_>>();
    if letter_atoms.len() == 1
        && let AtomView::Fun(list) = letter_atoms[0]
        && list.get_symbol() == Symbol::ARG
    {
        letter_atoms = list.iter().collect();
    }
    let letters = letter_atoms
        .into_iter()
        .map(|letter| Rat::from_atom(ctx.clone(), letter))
        .collect::<Result<Vec<_>>>()?;
    Ok(Expr::hlog(argument, Word::new(letters)))
}

fn lower(atom: AtomView<'_>, ctx: &Arc<PolyCtx>) -> Result<Expr> {
    let hlog = heads().hlog;
    match atom {
        AtomView::Fun(function) if function.get_symbol() == hlog => lower_hlog(function, ctx),
        AtomView::Add(addition) => addition
            .into_iter()
            .map(|child| lower(child, ctx))
            .collect::<Result<Vec<_>>>()
            .map(Expr::plus),
        AtomView::Mul(product) => product
            .into_iter()
            .map(|child| lower(child, ctx))
            .collect::<Result<Vec<_>>>()
            .map(Expr::times),
        AtomView::Pow(power) if power.get_base().contains_symbol(hlog) => Expr::power(
            lower(power.get_base(), ctx)?,
            positive_integer(power.get_exp())?,
        ),
        _ if !atom.contains_symbol(hlog) => Rat::from_atom(ctx.clone(), atom).map(Expr::leaf),
        _ => Err(Error::InvalidInput(format!(
            "unsupported Atom node in hyperlogarithm input: `{atom}`"
        ))),
    }
}

/// Lower a general Symbolica expression into the conversion core.
///
/// `Hlog(z, a1, ..., an)` is the canonical Atom representation. For callers
/// that already use Symbolica's `arg(...)` list head, `Hlog(z, arg(a1,...))`
/// is accepted as an equivalent spelling.
pub fn expression_from_atom(
    atom: AtomView<'_>,
    integration_variables: &[Symbol],
) -> Result<AtomExpression> {
    expression_from_atom_with_indeterminates(atom, integration_variables, &[])
}

/// Lower an Atom while reserving additional registered indeterminates.
///
/// This is used for exact constants such as `MZV(...)` that may be introduced
/// later by period reduction but do not necessarily occur in the input.
pub fn expression_from_atom_with_indeterminates(
    atom: AtomView<'_>,
    integration_variables: &[Symbol],
    additional_indeterminates: &[Atom],
) -> Result<AtomExpression> {
    let indeterminates =
        ordered_indeterminates(atom, integration_variables, additional_indeterminates);
    let ctx = PolyCtx::from_indeterminates(indeterminates.clone())?;
    let expr = lower(atom, &ctx)?;
    Ok(AtomExpression {
        expr,
        ctx,
        indeterminates,
    })
}

#[cfg(test)]
mod tests {
    use symbolica::prelude::{function, parse, symbol};

    use super::*;

    #[test]
    fn atom_hlog_is_lowered_without_string_reparsing() {
        let x = symbol!("atom_input_x");
        let input = heads().hlog.call((x, 0, -1));
        let lowered = expression_from_atom(input.as_view(), &[x]).unwrap();
        assert_eq!(lowered.expr.to_string(), "Hlog[atom_input_x,[0,-1]]");
        assert_eq!(lowered.ctx.index_of_symbol(x), Some(0));
    }

    #[test]
    fn symbolic_addition_and_power_preserve_hlog_structure() {
        let x = symbol!("atom_power_x");
        let hlog = function!(heads().hlog, x, 0);
        let input = parse!("3") * &hlog + hlog.pow(2);
        let lowered = expression_from_atom(input.as_view(), &[x]).unwrap();
        assert!(matches!(lowered.expr, Expr::Plus(_)));
    }

    #[test]
    fn registered_future_indeterminates_are_reserved_without_reparsing() {
        let x = symbol!("atom_reserved_x");
        let mzv = crate::symbols::mzv_atom(&[2, 1]);
        let lowered = expression_from_atom_with_indeterminates(
            x.to_atom().as_view(),
            &[x],
            std::slice::from_ref(&mzv),
        )
        .unwrap();
        assert_eq!(lowered.indeterminates, vec![x.to_atom(), mzv.clone()]);
        assert_eq!(lowered.ctx.variable_atom(1).unwrap(), mzv);
    }

    #[test]
    fn arg_wrapped_hlog_letters_are_collected_as_letters_not_as_one_list() {
        let (x, a, b) = symbol!("atom_arg_list_x", "atom_arg_list_a", "atom_arg_list_b");
        let list = Symbol::ARG.call((a, b));
        let mut collected = HashSet::new();
        collect_hlog_argument_indeterminates(list.as_view(), &mut collected);
        assert_eq!(collected, HashSet::from([a.to_atom(), b.to_atom()]));
        assert!(!collected.contains(&list));

        // Symbolica's public constructor flattens `arg(...)`; lowering still
        // accepts the documented compatibility spelling without reparsing.
        let input = heads().hlog.call((x, list));
        let lowered = expression_from_atom(input.as_view(), &[x]).unwrap();
        assert_eq!(
            lowered.expr.to_string(),
            "Hlog[atom_arg_list_x,[atom_arg_list_a,atom_arg_list_b]]"
        );
        assert!(lowered.ctx.index_of_symbol(a).is_some());
        assert!(lowered.ctx.index_of_symbol(b).is_some());
    }
}
