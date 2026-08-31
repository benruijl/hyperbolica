use std::sync::Arc;

use super::FactoredRat;
use crate::core::{Poly, PolyCtx, Rat};

fn context() -> Arc<PolyCtx> {
    PolyCtx::new(["x", "y", "z"]).unwrap()
}

fn value_equal(left: &Rat, right: &Rat) -> bool {
    left.numerator()
        .try_mul(right.denominator())
        .unwrap()
        .equal(&right.numerator().try_mul(left.denominator()).unwrap())
}

fn pseudo_random_poly(ctx: Arc<PolyCtx>, seed: u64) -> Poly {
    let x = Poly::generator(ctx.clone(), 0).unwrap();
    let y = Poly::generator(ctx.clone(), 1).unwrap();
    let z = Poly::generator(ctx.clone(), 2).unwrap();
    let mut result = Poly::from_int(ctx.clone(), 1 + (seed % 5) as i64);
    if seed & 1 != 0 {
        result = result.try_add(&x).unwrap();
    }
    if seed & 2 != 0 {
        result = result
            .try_add(&y.try_mul(&Poly::from_int(ctx.clone(), 2)).unwrap())
            .unwrap();
    }
    if seed & 4 != 0 {
        result = result.try_add(&z).unwrap();
    }
    if seed & 8 != 0 {
        result = result.try_add(&x.try_mul(&y).unwrap()).unwrap();
    }
    result
}

#[test]
fn parse_defers_denominator_power_and_preserves_value() {
    let ctx = context();
    for expression in [
        "(x + y)^2/(1 + x)^3",
        "(x + y)/(1 + x)",
        "x + y",
        "1/(1 + x)^4",
        "1/2*x + y",
        "x/((x + y)*(1 + x))",
    ] {
        let factored = FactoredRat::parse(ctx.clone(), expression).unwrap();
        let ordinary = Rat::parse(ctx.clone(), expression).unwrap();
        assert!(value_equal(&factored.materialize().unwrap(), &ordinary));
    }

    let factored = FactoredRat::parse(ctx, "(x + y)^2/(1 + x)^3").unwrap();
    assert_eq!(factored.den_factors().len(), 1);
    assert_eq!(factored.den_factors()[0].exp, 3);
    assert_eq!(factored.den_factors()[0].base.n_terms(), 2);
}

#[test]
fn factors_merge_and_known_powers_peel_exactly() {
    let ctx = context();
    let base = Poly::parse(ctx.clone(), "x+y+1").unwrap();
    let other = Poly::parse(ctx.clone(), "y+3").unwrap();
    let numerator = base
        .pow(2)
        .try_mul(&Poly::parse(ctx.clone(), "x+2").unwrap())
        .unwrap();
    let mut factored = FactoredRat::from_poly(numerator);
    factored.push_factor(&base, 2).unwrap();
    factored.push_factor(&base, 3).unwrap();
    factored.push_factor(&other, 1).unwrap();
    assert_eq!(factored.den_factors().len(), 2);
    assert_eq!(factored.den_factors()[0].exp, 5);

    let before = factored.materialize().unwrap();
    assert_eq!(factored.peel_known_factors(1).unwrap(), 2);
    assert_eq!(factored.den_factors()[0].exp, 3);
    assert!(!base.divides(factored.numerator()).unwrap());
    assert!(value_equal(&factored.materialize().unwrap(), &before));
}

#[test]
fn arithmetic_and_derivative_match_materialized_oracle() {
    let ctx = context();
    let left = Rat::parse(ctx.clone(), "(x+z)/(x+1)^2").unwrap();
    let right = Rat::parse(ctx.clone(), "(y+2)/(x*y+1)").unwrap();
    let factored_left = FactoredRat::from_rat(&left);
    let factored_right = FactoredRat::from_rat(&right);

    let cases = [
        (
            factored_left.try_add(&factored_right).unwrap(),
            left.try_add(&right).unwrap(),
        ),
        (
            factored_left.try_sub(&factored_right).unwrap(),
            left.try_sub(&right).unwrap(),
        ),
        (
            factored_left.try_mul(&factored_right).unwrap(),
            left.try_mul(&right).unwrap(),
        ),
        (
            factored_left.try_div(&factored_right).unwrap(),
            left.try_div(&right).unwrap(),
        ),
        (factored_left.pow(-3).unwrap(), left.pow(-3).unwrap()),
        (
            factored_left.derivative(0).unwrap(),
            left.derivative(0).unwrap(),
        ),
    ];
    for (factored, oracle) in cases {
        assert!(value_equal(&factored.materialize().unwrap(), &oracle));
    }

    let independent = FactoredRat::parse(ctx, "(x^2+z)/(y+1)^3").unwrap();
    let derivative = independent.derivative(0).unwrap();
    assert_eq!(derivative.den_factors()[0].exp, 3);
}

#[test]
fn deterministic_randomized_operations_are_value_equivalent() {
    let ctx = context();
    for seed in 1..48 {
        let left = Rat::new(
            pseudo_random_poly(ctx.clone(), seed),
            pseudo_random_poly(ctx.clone(), seed + 7),
        )
        .unwrap();
        let right = Rat::new(
            pseudo_random_poly(ctx.clone(), seed + 13),
            pseudo_random_poly(ctx.clone(), seed + 23),
        )
        .unwrap();
        let factored_left = FactoredRat::from_rat(&left);
        let factored_right = FactoredRat::from_rat(&right);

        let comparisons = [
            (
                factored_left.try_mul(&factored_right).unwrap(),
                left.try_mul(&right).unwrap(),
            ),
            (
                factored_left.try_add(&factored_right).unwrap(),
                left.try_add(&right).unwrap(),
            ),
            (
                factored_left.try_sub(&factored_right).unwrap(),
                left.try_sub(&right).unwrap(),
            ),
            (
                factored_left.try_div(&factored_right).unwrap(),
                left.try_div(&right).unwrap(),
            ),
            (factored_left.pow(3).unwrap(), left.pow(3).unwrap()),
            (
                factored_left.derivative((seed % 3) as usize).unwrap(),
                left.derivative((seed % 3) as usize).unwrap(),
            ),
        ];
        for (factored, oracle) in comparisons {
            assert!(value_equal(&factored.materialize().unwrap(), &oracle));
        }

        let chain = factored_left
            .try_mul(&factored_right)
            .unwrap()
            .try_add(&factored_left)
            .unwrap()
            .try_div(&factored_right)
            .unwrap();
        let oracle = left
            .try_mul(&right)
            .unwrap()
            .try_add(&left)
            .unwrap()
            .try_div(&right)
            .unwrap();
        assert!(value_equal(&chain.materialize().unwrap(), &oracle));
    }
}
