//! Checked introduction of formal quadratic-root letters.
//!
//! Symbolica supplies the factorization and exact polynomial arithmetic. This
//! module only preserves HyperFLINT's domain convention: the two roots of a
//! parameter-dependent quadratic are represented by the registered function
//! indeterminates `Wm(i)` and `Wp(i)`.

use crate::algebra::algebraic_letters::{
    AlgebraicLetterTable, algebraic_letters_allocate, join_algebraic_letter_session,
};
use crate::core::{Poly, Rat};
use crate::error::{Error, Result};

/// A quadratic factor after it has been admitted to the formal-letter table.
#[derive(Clone, Debug)]
pub(crate) struct IntroducedQuadratic {
    pub minus: Rat,
    pub plus: Rat,
}

/// Whether `polynomial` depends on a variable that must remain unintegrated.
pub(crate) fn depends_on_forbidden_variable(
    polynomial: &Poly,
    integration_variable: usize,
    forbidden_variables: &[usize],
) -> bool {
    let used = polynomial.used_variable_indices();
    forbidden_variables
        .iter()
        .any(|&forbidden| forbidden != integration_variable && used.contains(&forbidden))
}

/// Allocate the formal roots of a quadratic, or return `None` when the
/// remaining-variable guard rejects it.
pub(crate) fn introduce_quadratic(
    polynomial: &Poly,
    variable: usize,
    forbidden_variables: &[usize],
) -> Result<Option<IntroducedQuadratic>> {
    let _session = join_algebraic_letter_session()?;
    if polynomial.degree(variable)? != 2 {
        return Err(Error::InvalidInput(format!(
            "algebraic-letter introduction requires a quadratic in `{}`",
            polynomial.ctx().vars()[variable]
        )));
    }
    if depends_on_forbidden_variable(polynomial, variable, forbidden_variables) {
        return Ok(None);
    }

    // Symbolica's factor order/sign is deterministic in the pinned snapshot,
    // but the public letter identity must be invariant under a unit `-1`.
    let canonical = if polynomial.leading_coefficient_is_negative() {
        -polynomial
    } else {
        polynomial.clone()
    };
    let index = algebraic_letters_allocate(&canonical, variable)?;
    let entry = AlgebraicLetterTable::global().at(index)?;
    let atoms = entry.atoms();
    let minus_index = polynomial
        .ctx()
        .index_of_indeterminate(atoms.minus.as_view())
        .ok_or_else(|| missing_pool_error(index, "Wm"))?;
    let plus_index = polynomial
        .ctx()
        .index_of_indeterminate(atoms.plus.as_view())
        .ok_or_else(|| missing_pool_error(index, "Wp"))?;

    Ok(Some(IntroducedQuadratic {
        minus: Rat::from_poly(Poly::generator(polynomial.ctx().clone(), minus_index)?),
        plus: Rat::from_poly(Poly::generator(polynomial.ctx().clone(), plus_index)?),
    }))
}

fn missing_pool_error(index: usize, head: &str) -> Error {
    Error::InvalidInput(format!(
        "algebraic-letter pair {index} requires registered `{head}({index})` in the polynomial context; construct it with build_algebraic_letter_atom_list"
    ))
}

#[cfg(test)]
mod tests {
    use symbolica::prelude::{
        AtomCore, IntegerRing, PolyVariable, RationalPolynomialField, Root, Symbol,
    };

    use super::*;
    use crate::algebra::algebraic_letters::{
        DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE, begin_algebraic_letter_session,
        build_algebraic_letter_atom_list,
    };
    use crate::symbols::SYMBOL_NAMESPACE;

    fn context() -> std::sync::Arc<crate::core::PolyCtx> {
        let x = Symbol::parse("intro_x", SYMBOL_NAMESPACE).unwrap();
        let y = Symbol::parse("intro_y", SYMBOL_NAMESPACE).unwrap();
        crate::core::PolyCtx::from_indeterminates(build_algebraic_letter_atom_list(
            [x.to_atom(), y.to_atom()],
            DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE,
        ))
        .unwrap()
    }

    #[test]
    fn canonicalizes_sign_and_reuses_the_registered_pair() {
        let _session = begin_algebraic_letter_session().unwrap();
        let ctx = context();
        let positive = Poly::parse(ctx.clone(), "intro_x^2-intro_y").unwrap();
        let negative = -&positive;

        let first = introduce_quadratic(&positive, 0, &[]).unwrap().unwrap();
        let second = introduce_quadratic(&negative, 0, &[]).unwrap().unwrap();
        assert_eq!(first.minus, second.minus);
        assert_eq!(first.plus, second.plus);
        assert_eq!(AlgebraicLetterTable::global().size().unwrap(), 1);
    }

    #[test]
    fn remaining_variable_guard_does_not_mutate_the_table() {
        let _session = begin_algebraic_letter_session().unwrap();
        let ctx = context();
        let polynomial = Poly::parse(ctx, "intro_x^2-intro_y").unwrap();
        assert!(introduce_quadratic(&polynomial, 0, &[1]).unwrap().is_none());
        assert_eq!(AlgebraicLetterTable::global().size().unwrap(), 0);
    }

    #[test]
    fn symbolica_parametric_root_is_the_branch_formula_oracle() {
        let _session = begin_algebraic_letter_session().unwrap();
        let ctx = context();
        let polynomial = Poly::parse(ctx.clone(), "intro_x^2-intro_y").unwrap();
        let root_variable = PolyVariable::try_from(ctx.variable_atom(0).unwrap()).unwrap();
        type ParameterRoot = Root<RationalPolynomialField<IntegerRing, u16>>;

        let minus = ParameterRoot::from_atom_with_variable(
            polynomial.to_atom().as_view(),
            root_variable.clone(),
            0,
        )
        .unwrap()
        .simplify()
        .unwrap();
        let plus = ParameterRoot::from_atom_with_variable(
            polynomial.to_atom().as_view(),
            root_variable,
            1,
        )
        .unwrap()
        .simplify()
        .unwrap();
        assert!((minus.clone() + plus.clone()).expand().is_zero());
        assert_eq!(
            (minus * plus).expand(),
            (-ctx.variable_atom(1).unwrap()).expand()
        );
    }
}
