use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use symbolica::prelude::*;

use super::{Poly, PolyCtx};
use crate::error::Error;

fn context() -> Arc<PolyCtx> {
    PolyCtx::new(["x", "y"]).unwrap()
}

#[test]
fn arithmetic_and_exact_division() {
    let ctx = context();
    let x_plus_y = Poly::parse(ctx.clone(), "x+y").unwrap();
    let x_minus_y = Poly::parse(ctx.clone(), "x-y").unwrap();
    let product = &x_plus_y * &x_minus_y;
    let expected = Poly::parse(ctx, "x^2-y^2").unwrap();
    assert_eq!(product, expected);
    assert_eq!(product.div_exact(&x_plus_y).unwrap(), x_minus_y);
}

#[test]
fn structural_hash_matches_polynomial_and_context_equality() {
    let first_ctx = context();
    let equivalent_ctx = context();
    let expanded = Poly::parse(first_ctx, "(x+y)^4").unwrap();
    let canonical = Poly::parse(equivalent_ctx, "x^4+4*x^3*y+6*x^2*y^2+4*x*y^3+y^4").unwrap();
    assert_eq!(expanded, canonical);

    let hash = |polynomial: &Poly| {
        let mut state = DefaultHasher::new();
        polynomial.hash(&mut state);
        state.finish()
    };
    assert_eq!(hash(&expanded), hash(&canonical));

    let different_context = PolyCtx::new(["y", "x"]).unwrap();
    let constant_left = Poly::from_int(expanded.ctx().clone(), 3);
    let constant_right = Poly::from_int(different_context, 3);
    assert_ne!(constant_left, constant_right);
}

#[test]
fn context_compatibility_includes_namespaced_symbolica_variables() {
    let left_symbol = Symbol::parse("x", "context_compatibility_left").unwrap();
    let right_symbol = Symbol::parse("x", "context_compatibility_right").unwrap();
    let left = PolyCtx::from_indeterminates([left_symbol.to_atom()]).unwrap();
    let right = PolyCtx::from_indeterminates([right_symbol.to_atom()]).unwrap();

    // PolyVariable's diagnostic spelling deliberately strips namespaces.
    // Ring compatibility must still retain Symbolica's structural identity.
    assert_eq!(left.vars(), right.vars());
    assert!(!left.is_compatible_with(&right));
    assert_ne!(Poly::one(left), Poly::one(right));
}

#[test]
fn context_compatibility_ignores_constructor_specific_diagnostic_spelling() {
    let symbol = Symbol::parse("x", "context_compatibility_shared").unwrap();
    let qualified = PolyCtx::from_symbols([symbol]).unwrap();
    let stripped = PolyCtx::from_indeterminates([symbol.to_atom()]).unwrap();

    assert_ne!(qualified.vars(), stripped.vars());
    assert!(qualified.is_compatible_with(&stripped));

    let qualified_x = Poly::generator(qualified, 0).unwrap();
    let stripped_x = Poly::generator(stripped, 0).unwrap();
    assert_eq!(qualified_x, stripped_x);

    let hash = |polynomial: &Poly| {
        let mut state = DefaultHasher::new();
        polynomial.hash(&mut state);
        state.finish()
    };
    assert_eq!(hash(&qualified_x), hash(&stripped_x));
}

#[test]
fn improved_symbolica_resultant_is_used() {
    let ctx = context();
    let left = Poly::parse(ctx.clone(), "x^2+y*x+1").unwrap();
    let right = Poly::parse(ctx.clone(), "x-y").unwrap();
    let resultant = left.resultant(&right, 0).unwrap();
    assert_eq!(resultant, Poly::parse(ctx, "2*y^2+1").unwrap());
}

#[test]
fn coefficients_and_discriminant() {
    let ctx = context();
    let polynomial = Poly::parse(ctx.clone(), "y*x^2+3*x+1").unwrap();
    assert_eq!(
        polynomial.coefficient_of(0, 2).unwrap(),
        Poly::parse(ctx.clone(), "y").unwrap()
    );
    assert_eq!(
        polynomial.discriminant(0).unwrap(),
        Poly::parse(ctx.clone(), "9-4*y").unwrap()
    );
    assert_eq!(
        polynomial.resultant_discriminant(0).unwrap(),
        Poly::parse(ctx, "4*y-9").unwrap()
    );
}

#[test]
fn standard_discriminant_has_the_degree_dependent_sign() {
    let ctx = context();
    let cubic = Poly::parse(ctx.clone(), "x^3+y*x+1").unwrap();
    assert_eq!(
        cubic.discriminant(0).unwrap(),
        Poly::parse(ctx.clone(), "-4*y^3-27").unwrap()
    );
    assert_eq!(
        cubic.resultant_discriminant(0).unwrap(),
        Poly::parse(ctx, "4*y^3+27").unwrap()
    );
}

#[test]
fn typed_substitution_and_full_evaluation_are_exact() {
    let ctx = context();
    let polynomial = Poly::parse(ctx.clone(), "3*x^9*y^4-2*x^2*y+7").unwrap();
    let half = Rational::new(1, 2);

    assert_eq!(
        polynomial.substitute_rational(0, &half).unwrap(),
        Poly::parse(ctx.clone(), "3/512*y^4-y/2+7").unwrap()
    );
    assert_eq!(
        polynomial
            .substitute_integer(1, &Integer::from(-2))
            .unwrap(),
        Poly::parse(ctx.clone(), "48*x^9+4*x^2+7").unwrap()
    );
    assert_eq!(
        polynomial
            .evaluate_rational(&[half, Rational::new(1, 3)])
            .unwrap(),
        Rational::new(94_465, 13_824)
    );
    assert_eq!(
        polynomial
            .evaluate_integer(&[Integer::from(2), Integer::from(-1)])
            .unwrap(),
        Rational::from(1_551)
    );

    assert!(matches!(
        polynomial.substitute_rational(2, &Rational::one()),
        Err(Error::UnknownVariable(_))
    ));
    assert!(matches!(
        polynomial.evaluate_rational(&[Rational::one()]),
        Err(Error::InvalidInput(_))
    ));

    let zero = Poly::zero(ctx);
    assert!(
        zero.substitute_integer(0, &Integer::from(37))
            .unwrap()
            .is_zero()
    );
    assert_eq!(
        zero.evaluate_rational(&[Rational::new(2, 3), Rational::new(-5, 7)])
            .unwrap(),
        Rational::zero()
    );
    assert!(zero.integrate(0).unwrap().is_zero());
}

#[test]
fn rational_constants_and_factor_units_stay_typed() {
    let ctx = context();
    let value = Rational::new(-7, 12);
    let constant = Poly::from_rational(ctx.clone(), value.clone());
    assert_eq!(constant.rational_constant(), Some(value.clone()));
    let constant_factorization = constant.factor();
    assert_eq!(constant_factorization.constant, value);
    assert!(constant_factorization.factors.is_empty());

    let polynomial = Poly::parse(ctx.clone(), "-7/12*(x+y)^2*(x-1)").unwrap();
    let factorization = polynomial.factor();
    assert!(!factorization.constant.is_zero());

    let mut reconstructed = Poly::from_rational(ctx, factorization.constant);
    for (factor, multiplicity) in factorization.factors {
        reconstructed = reconstructed.try_mul(&factor.pow(multiplicity)).unwrap();
    }
    assert_eq!(reconstructed, polynomial);
}

#[test]
fn native_integral_round_trips_and_sparse_coefficients_stay_sparse() {
    let ctx = context();
    let polynomial = Poly::parse(ctx.clone(), "11*x^60000*y^7-5*x^41*y+3*x^2+17").unwrap();

    assert_eq!(
        polynomial.coefficient_of(0, 60_000).unwrap(),
        Poly::parse(ctx.clone(), "11*y^7").unwrap()
    );
    assert!(polynomial.coefficient_of(0, 59_999).unwrap().is_zero());
    assert!(polynomial.coefficient_of(0, -1).unwrap().is_zero());

    let primitive = polynomial.integrate(0).unwrap();
    assert_eq!(primitive.derivative(0).unwrap(), polynomial);
    assert_eq!(
        primitive,
        Poly::parse(ctx.clone(), "11/60001*x^60001*y^7-5/42*x^42*y+x^3+17*x",).unwrap()
    );

    let maximum = Poly::parse(ctx, "x^65535").unwrap();
    assert!(matches!(maximum.integrate(0), Err(Error::InvalidInput(_))));
}

#[test]
fn transplant_uses_native_rearrangement_and_preserves_explicit_identification() {
    let source = context();
    let destination = PolyCtx::new(["y", "z", "x"]).unwrap();
    let polynomial = Poly::parse(source.clone(), "x^2*y+3*y+1").unwrap();
    assert_eq!(
        polynomial
            .transplant(destination.clone(), &[Some(2), Some(0)])
            .unwrap(),
        Poly::parse(destination, "x^2*y+3*y+1").unwrap()
    );

    let identified_ctx = PolyCtx::new(["z"]).unwrap();
    let identified = Poly::parse(source, "x*y+x+y").unwrap();
    assert_eq!(
        identified
            .transplant(identified_ctx.clone(), &[Some(0), Some(0)])
            .unwrap(),
        Poly::parse(identified_ctx.clone(), "z^2+2*z").unwrap()
    );
    assert!(matches!(
        identified.transplant(identified_ctx, &[Some(0), None]),
        Err(Error::InvalidInput(_))
    ));
}
