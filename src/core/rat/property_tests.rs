use symbolica::prelude::{Field, Q, Rational, RingOps};

use super::Rat;
use crate::core::{Poly, PolyCtx};
use crate::error::Error;

struct SparseCase<'a> {
    left_num: &'a str,
    left_den: &'a str,
    right_num: &'a str,
    right_den: &'a str,
    point: [(i64, i64); 3],
}

#[test]
fn deterministic_sparse_arithmetic_reconstructs_and_evaluates_exactly() {
    let cases = [
        SparseCase {
            left_num: "x^7+2*y^3-z+1",
            left_den: "1+x*y+z^2",
            right_num: "3*x^2*z-y+2",
            right_den: "2+x^3+y*z",
            point: [(1, 2), (-1, 1), (2, 1)],
        },
        SparseCase {
            left_num: "2*x*y^5+z^4-3",
            left_den: "3+x^2+2*y*z",
            right_num: "x^11-y^2*z+5",
            right_den: "1-x+z^3",
            point: [(2, 1), (1, 3), (-1, 1)],
        },
        // The common sparse factor checks reconstruction after cancellation.
        SparseCase {
            left_num: "(x+y)*(x^5+z)",
            left_den: "(x+y)*(1+x*z)",
            right_num: "x-y+z",
            right_den: "4+x*y*z+x^4",
            point: [(-1, 1), (2, 1), (1, 2)],
        },
    ];

    let ctx = PolyCtx::new(["x", "y", "z"]).unwrap();
    for (case_index, case) in cases.iter().enumerate() {
        let left_num = Poly::parse(ctx.clone(), case.left_num).unwrap();
        let left_den = Poly::parse(ctx.clone(), case.left_den).unwrap();
        let right_num = Poly::parse(ctx.clone(), case.right_num).unwrap();
        let right_den = Poly::parse(ctx.clone(), case.right_den).unwrap();
        let left = Rat::new(left_num.clone(), left_den.clone()).unwrap();
        let right = Rat::new(right_num.clone(), right_den.clone()).unwrap();

        for (value, numerator, denominator, side) in [
            (&left, &left_num, &left_den, "left"),
            (&right, &right_num, &right_den, "right"),
        ] {
            assert_eq!(
                value.try_mul(&Rat::from_poly(denominator.clone())).unwrap(),
                Rat::from_poly(numerator.clone()),
                "input reconstruction, case {case_index}, {side}"
            );
        }

        let sum = left.try_add(&right).unwrap();
        let difference = left.try_sub(&right).unwrap();
        let product = left.try_mul(&right).unwrap();
        let quotient = left.try_div(&right).unwrap();
        assert_eq!(sum.try_sub(&right).unwrap(), left, "case {case_index}");
        assert_eq!(
            difference.try_add(&right).unwrap(),
            left,
            "case {case_index}"
        );
        assert_eq!(product.try_div(&right).unwrap(), left, "case {case_index}");
        assert_eq!(quotient.try_mul(&right).unwrap(), left, "case {case_index}");
        assert_eq!(
            left.pow(3).unwrap(),
            left.try_mul(&left).unwrap().try_mul(&left).unwrap(),
            "positive power, case {case_index}"
        );
        assert_eq!(
            left.pow(-2)
                .unwrap()
                .try_mul(&left.pow(2).unwrap())
                .unwrap(),
            Rat::one(ctx.clone()),
            "negative power, case {case_index}"
        );

        for (value, operation) in [
            (&left, "left"),
            (&right, "right"),
            (&sum, "sum"),
            (&difference, "difference"),
            (&product, "product"),
            (&quotient, "quotient"),
        ] {
            assert_eq!(
                Rat::new(value.numerator().clone(), value.denominator().clone()).unwrap(),
                *value,
                "canonical num/den reconstruction, case {case_index}, {operation}"
            );
            assert_eq!(
                Rat::from_atom(ctx.clone(), value.to_atom().as_view()).unwrap(),
                *value,
                "Atom reconstruction, case {case_index}, {operation}"
            );
        }

        let numerator = left.numerator().clone();
        let denominator = left.denominator().clone();
        let expected_derivative = Rat::new(
            numerator
                .derivative(0)
                .unwrap()
                .try_mul(&denominator)
                .unwrap()
                .try_sub(
                    &numerator
                        .try_mul(&denominator.derivative(0).unwrap())
                        .unwrap(),
                )
                .unwrap(),
            denominator.pow(2),
        )
        .unwrap();
        assert_eq!(
            left.derivative(0).unwrap(),
            expected_derivative,
            "quotient rule, case {case_index}"
        );

        let point = case
            .point
            .map(|(numerator, denominator)| Rational::new(numerator, denominator));
        let left_value = left.evaluate_rational(&point).unwrap();
        let right_value = right.evaluate_rational(&point).unwrap();
        assert_eq!(
            sum.evaluate_rational(&point).unwrap(),
            Q.add(&left_value, &right_value),
            "sum evaluation, case {case_index}"
        );
        assert_eq!(
            difference.evaluate_rational(&point).unwrap(),
            Q.sub(&left_value, &right_value),
            "difference evaluation, case {case_index}"
        );
        assert_eq!(
            product.evaluate_rational(&point).unwrap(),
            Q.mul(&left_value, &right_value),
            "product evaluation, case {case_index}"
        );
        assert_eq!(
            quotient.evaluate_rational(&point).unwrap(),
            Q.div(&left_value, &right_value),
            "quotient evaluation, case {case_index}"
        );

        let scalar_replacement = Rational::new(2, 3);
        let substituted = left.substitute_rational(0, &scalar_replacement).unwrap();
        let scalar_point = [scalar_replacement, point[1].clone(), point[2].clone()];
        assert_eq!(
            substituted
                .evaluate_rational(&[Rational::zero(), point[1].clone(), point[2].clone()])
                .unwrap(),
            left.evaluate_rational(&scalar_point).unwrap(),
            "scalar substitution, case {case_index}"
        );

        let rational_replacement = Rat::parse(ctx.clone(), "(y+1)/(z+2)").unwrap();
        let composed = left.substitute_rat(0, &rational_replacement).unwrap();
        let mut composed_point = point.clone();
        composed_point[0] = rational_replacement.evaluate_rational(&point).unwrap();
        assert_eq!(
            composed.evaluate_rational(&point).unwrap(),
            left.evaluate_rational(&composed_point).unwrap(),
            "rational substitution, case {case_index}"
        );

        assert!(matches!(
            left.evaluate_rational(&point[..2]),
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            left.substitute_rational(3, &Rational::one()),
            Err(Error::UnknownVariable(_))
        ));
        assert!(matches!(
            left.try_div(&Rat::zero(ctx.clone())),
            Err(Error::DivisionByZero)
        ));
    }
}
