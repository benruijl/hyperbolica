use super::*;

use symbolica::prelude::{AtomCore, Symbol};

use crate::algebra::algebraic_letters::{
    AlgebraicLetterTable, DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE, begin_algebraic_letter_session,
    build_algebraic_letter_atom_list,
};
use crate::core::{Poly, PolyCtx};
use crate::symbols::SYMBOL_NAMESPACE;

fn reconstruct(result: &PartialFractionization, variable: usize) -> Rat {
    let ctx = result.polynomial_part.ctx().clone();
    let x = Rat::from_poly(Poly::generator(ctx, variable).unwrap());
    let mut value = result.polynomial_part.clone();
    for pole in &result.poles {
        assert_eq!(pole.coefs.len(), pole.multiplicity);
        let base = x.try_sub(&pole.pole).unwrap();
        for (order, coefficient) in pole.coefs.iter().enumerate() {
            let term = coefficient
                .try_div(&base.pow((order + 1) as i64).unwrap())
                .unwrap();
            value = value.try_add(&term).unwrap();
        }
    }
    value
}

fn assert_cross_product_equal(left: &Rat, right: &Rat) {
    assert_eq!(
        left.native().numerator.clone() * &right.native().denominator,
        right.native().numerator.clone() * &left.native().denominator
    );
}

fn algebraic_context() -> std::sync::Arc<PolyCtx> {
    let variables = ["pf_alg_x", "pf_alg_a", "pf_alg_b", "pf_alg_z"]
        .into_iter()
        .map(|name| Symbol::parse(name, SYMBOL_NAMESPACE).unwrap().to_atom());
    PolyCtx::from_indeterminates(build_algebraic_letter_atom_list(
        variables,
        DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE,
    ))
    .unwrap()
}

fn algebraic_options<'a>(forbidden_variables: &'a [usize]) -> PartialFractionOptions<'a> {
    PartialFractionOptions {
        introduce_algebraic_letters: true,
        forbidden_variables,
    }
}

#[test]
fn simple_poles_and_polynomial_part_reconstruct() {
    let ctx = PolyCtx::new(["x"]).unwrap();
    let function = Rat::parse(ctx, "(x^3+2*x+1)/((x-1)*(x+2))").unwrap();
    let result = partial_fractions(&function, 0).unwrap();

    assert_eq!(result.poles.len(), 2);
    assert!(result.poles.iter().all(|pole| pole.multiplicity == 1));
    assert_eq!(reconstruct(&result, 0), function);
}

#[test]
fn repeated_pole_coefficients_are_in_ascending_order() {
    let ctx = PolyCtx::new(["x", "y"]).unwrap();
    let function = Rat::parse(ctx.clone(), "(x^3+2*x+y)/(x-y)^2").unwrap();
    let result = partial_fractions(&function, 0).unwrap();

    assert_eq!(result.poles.len(), 1);
    assert_eq!(result.poles[0].pole, Rat::parse(ctx.clone(), "y").unwrap());
    assert_eq!(result.poles[0].multiplicity, 2);
    assert_eq!(
        result.poles[0].coefs,
        vec![
            Rat::parse(ctx.clone(), "3*y^2+2").unwrap(),
            Rat::parse(ctx.clone(), "y^3+3*y").unwrap(),
        ]
    );
    assert_eq!(result.polynomial_part, Rat::parse(ctx, "x+2*y").unwrap());
    assert_eq!(reconstruct(&result, 0), function);
}

#[test]
fn rational_parameter_pole_reconstructs_with_leading_coefficient_scaling() {
    let ctx = PolyCtx::new(["x", "y", "z"]).unwrap();
    let function = Rat::parse(ctx.clone(), "(x^2+y*z+z^2)/(z*x-y)^3").unwrap();
    let result = partial_fractions(&function, 0).unwrap();

    assert!(result.polynomial_part.is_zero());
    assert_eq!(result.poles.len(), 1);
    let pole = &result.poles[0];
    assert_eq!(pole.pole, Rat::parse(ctx.clone(), "y/z").unwrap());
    assert_eq!(pole.multiplicity, 3);
    assert_eq!(
        pole.coefs,
        vec![
            Rat::parse(ctx.clone(), "1/z^3").unwrap(),
            Rat::parse(ctx.clone(), "2*y/z^4").unwrap(),
            Rat::parse(ctx, "(y^2+y*z^3+z^4)/z^5").unwrap(),
        ]
    );
    assert_eq!(reconstruct(&result, 0), function);
}

#[test]
fn nonlinear_denominator_is_reported() {
    let ctx = PolyCtx::new(["x", "y"]).unwrap();
    let function = Rat::parse(ctx, "1/(x^2+y)").unwrap();
    let error = partial_fractions(&function, 0).unwrap_err();
    assert!(error.to_string().contains("degree 2"));
}

#[test]
fn zero_and_polynomial_inputs_have_only_a_polynomial_part() {
    let ctx = PolyCtx::new(["x", "y"]).unwrap();
    let zero = partial_fractions(&Rat::zero(ctx.clone()), 0).unwrap();
    assert!(zero.polynomial_part.is_zero());
    assert!(zero.poles.is_empty());

    let polynomial = Rat::parse(ctx.clone(), "x^4+y*x+1").unwrap();
    let result = partial_fractions(&polynomial, 0).unwrap();
    assert_eq!(result.polynomial_part, polynomial);
    assert!(result.poles.is_empty());

    let cancelled = Rat::parse(ctx, "(x^2-1)/(x-1)").unwrap();
    let result = partial_fractions(&cancelled, 0).unwrap();
    assert_eq!(result.polynomial_part, cancelled);
    assert!(result.poles.is_empty());
}

#[test]
fn several_scaled_repeated_poles_reconstruct_exactly() {
    let ctx = PolyCtx::new(["x", "y", "z"]).unwrap();
    let function =
        Rat::parse(ctx, "(x^7+y*x^5+z*x^3+y*z*x+1)/((2*x-y)^3*(x+z)^2*(3*x+1))").unwrap();
    let result = partial_fractions(&function, 0).unwrap();

    assert_eq!(result.poles.len(), 3);
    let mut multiplicities = result
        .poles
        .iter()
        .map(|pole| pole.multiplicity)
        .collect::<Vec<_>>();
    multiplicities.sort_unstable();
    assert_eq!(multiplicities, [1, 2, 3]);
    assert!(result.poles.iter().all(|pole| {
        pole.coefs.iter().all(|coefficient| {
            coefficient.numerator().degree(0).unwrap() <= 0
                && coefficient.denominator().degree(0).unwrap() <= 0
        })
    }));
    assert_eq!(reconstruct(&result, 0), function);
}

#[test]
fn mixed_repeated_function_indeterminate_poles_reconstruct_exactly() {
    let ctx = algebraic_context();
    let atoms = crate::symbols::algebraic_atoms(1);
    let minus_index = ctx.index_of_indeterminate(atoms.minus.as_view()).unwrap();
    let plus_index = ctx.index_of_indeterminate(atoms.plus.as_view()).unwrap();
    let x = Rat::from_poly(Poly::generator(ctx.clone(), 0).unwrap());
    let b = Rat::from_poly(Poly::generator(ctx.clone(), 2).unwrap());
    let minus = Rat::from_poly(Poly::generator(ctx.clone(), minus_index).unwrap());
    let plus = Rat::from_poly(Poly::generator(ctx.clone(), plus_index).unwrap());
    let numerator = Rat::parse(ctx.clone(), "pf_alg_x^4+pf_alg_a*pf_alg_x^2+pf_alg_b+1").unwrap();
    let denominator = x
        .try_sub(&minus)
        .unwrap()
        .pow(4)
        .unwrap()
        .try_mul(&x.try_sub(&plus).unwrap().pow(3).unwrap())
        .unwrap()
        .try_mul(&x.try_sub(&b).unwrap().pow(2).unwrap())
        .unwrap();
    let function = numerator.try_div(&denominator).unwrap();

    let result = partial_fractions(&function, 0).unwrap();
    let mut multiplicities = result
        .poles
        .iter()
        .map(|pole| pole.multiplicity)
        .collect::<Vec<_>>();
    multiplicities.sort_unstable();
    assert_eq!(multiplicities, [2, 3, 4]);

    let reconstructed = reconstruct(&result, 0);
    assert_cross_product_equal(&reconstructed, &function);
    assert_eq!(reconstructed, function);
}

#[test]
fn invalid_variable_is_reported_before_trivial_returns() {
    let ctx = PolyCtx::new(["x"]).unwrap();
    let error = partial_fractions(&Rat::zero(ctx), 1).unwrap_err();
    assert!(matches!(error, Error::UnknownVariable(variable) if variable == "1"));
}

#[test]
fn irreducible_quadratic_is_split_into_registered_function_poles() {
    let _session = begin_algebraic_letter_session().unwrap();
    let ctx = algebraic_context();
    let function = Rat::parse(ctx.clone(), "1/(pf_alg_x^2-pf_alg_a)").unwrap();
    let result = partial_fractions_with_options(&function, 0, &algebraic_options(&[])).unwrap();

    assert!(result.polynomial_part.is_zero());
    assert_eq!(result.poles.len(), 2);
    assert!(result.poles.iter().all(|pole| pole.multiplicity == 1));
    assert!(result.poles.iter().all(|pole| {
        let atom = pole.pole.to_atom();
        atom.as_fun_view().is_some()
    }));

    let x = Rat::from_poly(Poly::generator(ctx.clone(), 0).unwrap());
    let split = x
        .try_sub(&result.poles[0].pole)
        .unwrap()
        .try_mul(&x.try_sub(&result.poles[1].pole).unwrap())
        .unwrap();
    let expected = Rat::one(ctx).try_div(&split).unwrap();
    assert_eq!(reconstruct(&result, 0), expected);

    let entry = AlgebraicLetterTable::global().at(1).unwrap();
    assert_eq!(entry.polynomial, function.denominator().clone());
    assert_eq!(entry.sum_value, Rat::zero(entry.polynomial.ctx().clone()));
    assert_eq!(
        entry.product_value,
        Rat::parse(entry.polynomial.ctx().clone(), "-pf_alg_a").unwrap()
    );
}

#[test]
fn repeated_quadratic_and_linear_factors_reconstruct_in_the_split_alphabet() {
    let _session = begin_algebraic_letter_session().unwrap();
    let ctx = algebraic_context();
    let numerator = Rat::parse(ctx.clone(), "pf_alg_x+pf_alg_b").unwrap();
    let function = Rat::parse(
        ctx.clone(),
        "(pf_alg_x+pf_alg_b)/((pf_alg_x^2-pf_alg_a)^2*(pf_alg_x-pf_alg_b))",
    )
    .unwrap();
    let result = partial_fractions_with_options(&function, 0, &algebraic_options(&[])).unwrap();

    assert_eq!(result.poles.len(), 3);
    let mut multiplicities = result
        .poles
        .iter()
        .map(|pole| pole.multiplicity)
        .collect::<Vec<_>>();
    multiplicities.sort_unstable();
    assert_eq!(multiplicities, [1, 2, 2]);

    let x = Rat::from_poly(Poly::generator(ctx.clone(), 0).unwrap());
    let algebraic = result
        .poles
        .iter()
        .filter(|pole| pole.multiplicity == 2)
        .collect::<Vec<_>>();
    let b = Rat::from_poly(Poly::generator(ctx.clone(), 2).unwrap());
    let split_denominator = x
        .try_sub(&algebraic[0].pole)
        .unwrap()
        .pow(2)
        .unwrap()
        .try_mul(&x.try_sub(&algebraic[1].pole).unwrap().pow(2).unwrap())
        .unwrap()
        .try_mul(&x.try_sub(&b).unwrap())
        .unwrap();
    let expected = numerator.try_div(&split_denominator).unwrap();
    let reconstructed = reconstruct(&result, 0);
    assert_cross_product_equal(&reconstructed, &expected);
    assert_eq!(reconstructed, expected);
}

#[test]
fn multiple_quadratics_with_a_repeated_pair_are_split_as_one_formal_rational_function() {
    let _session = begin_algebraic_letter_session().unwrap();
    let ctx = algebraic_context();
    let numerator = Rat::parse(ctx.clone(), "pf_alg_x+pf_alg_b").unwrap();
    let function = Rat::parse(
            ctx.clone(),
            "(pf_alg_x+pf_alg_b)/((pf_alg_x^2-pf_alg_a)^2*(pf_alg_z*pf_alg_x^2+pf_alg_x+pf_alg_b)*(pf_alg_x-pf_alg_b))",
        )
        .unwrap();
    let result = partial_fractions_with_options(&function, 0, &algebraic_options(&[])).unwrap();

    let mut multiplicities = result
        .poles
        .iter()
        .map(|pole| pole.multiplicity)
        .collect::<Vec<_>>();
    multiplicities.sort_unstable();
    assert_eq!(multiplicities, [1, 1, 1, 2, 2]);
    assert_eq!(AlgebraicLetterTable::global().size().unwrap(), 2);

    let x = Rat::from_poly(Poly::generator(ctx.clone(), 0).unwrap());
    let z = Rat::from_poly(Poly::generator(ctx.clone(), 3).unwrap());
    let mut split_denominator = z;
    for pole in &result.poles {
        split_denominator = split_denominator
            .try_mul(
                &x.try_sub(&pole.pole)
                    .unwrap()
                    .pow(pole.multiplicity as i64)
                    .unwrap(),
            )
            .unwrap();
    }
    let expected = numerator.try_div(&split_denominator).unwrap();
    let reconstructed = reconstruct(&result, 0);
    assert_cross_product_equal(&reconstructed, &expected);
    assert_eq!(reconstructed, expected);
}

#[test]
fn improper_multiple_quadratics_preserve_the_polynomial_part_before_splitting() {
    let _session = begin_algebraic_letter_session().unwrap();
    let ctx = algebraic_context();
    let function = Rat::parse(
        ctx.clone(),
        "pf_alg_x+1/((pf_alg_x^2-pf_alg_a)*(pf_alg_x^2-pf_alg_b))",
    )
    .unwrap();
    let result = partial_fractions_with_options(&function, 0, &algebraic_options(&[])).unwrap();

    assert_eq!(
        result.polynomial_part,
        Rat::parse(ctx.clone(), "pf_alg_x").unwrap()
    );
    assert_eq!(result.poles.len(), 4);
    assert!(result.poles.iter().all(|pole| pole.multiplicity == 1));
    assert_eq!(AlgebraicLetterTable::global().size().unwrap(), 2);

    let x = Rat::from_poly(Poly::generator(ctx.clone(), 0).unwrap());
    let mut split_denominator = Rat::one(ctx.clone());
    for pole in &result.poles {
        split_denominator = split_denominator
            .try_mul(&x.try_sub(&pole.pole).unwrap())
            .unwrap();
    }
    let expected = x
        .try_add(&Rat::one(ctx).try_div(&split_denominator).unwrap())
        .unwrap();
    assert_cross_product_equal(&reconstruct(&result, 0), &expected);
}

#[test]
fn repeated_linear_pole_keeps_structural_zero_coefficients() {
    let ctx = PolyCtx::new(["x", "a"]).unwrap();
    let function = Rat::parse(ctx.clone(), "1/(x-a)^4").unwrap();
    let result = partial_fractions(&function, 0).unwrap();

    assert_eq!(result.poles.len(), 1);
    assert_eq!(result.poles[0].multiplicity, 4);
    assert_eq!(
        result.poles[0].coefs,
        vec![
            Rat::zero(ctx.clone()),
            Rat::zero(ctx.clone()),
            Rat::zero(ctx.clone()),
            Rat::one(ctx),
        ]
    );
    assert_cross_product_equal(&reconstruct(&result, 0), &function);
}

#[test]
fn parameter_leading_coefficient_is_preserved_exactly() {
    let _session = begin_algebraic_letter_session().unwrap();
    let ctx = algebraic_context();
    let function = Rat::parse(ctx.clone(), "1/(pf_alg_z*pf_alg_x^2+pf_alg_x+pf_alg_a)").unwrap();
    let result = partial_fractions_with_options(&function, 0, &algebraic_options(&[])).unwrap();
    assert_eq!(result.poles.len(), 2);

    let x = Rat::from_poly(Poly::generator(ctx.clone(), 0).unwrap());
    let z = Rat::from_poly(Poly::generator(ctx.clone(), 3).unwrap());
    let split_denominator = z
        .try_mul(&x.try_sub(&result.poles[0].pole).unwrap())
        .unwrap()
        .try_mul(&x.try_sub(&result.poles[1].pole).unwrap())
        .unwrap();
    assert_eq!(
        reconstruct(&result, 0),
        Rat::one(ctx).try_div(&split_denominator).unwrap()
    );
}

#[test]
fn remaining_variable_dependency_is_a_clear_failure_without_allocation() {
    let _session = begin_algebraic_letter_session().unwrap();
    let ctx = algebraic_context();
    let function = Rat::parse(ctx, "1/(pf_alg_x^2-pf_alg_a)").unwrap();
    let error = partial_fractions_with_options(&function, 0, &algebraic_options(&[1])).unwrap_err();
    assert!(error.to_string().contains("remaining integration variable"));
    assert_eq!(AlgebraicLetterTable::global().size().unwrap(), 0);
}
