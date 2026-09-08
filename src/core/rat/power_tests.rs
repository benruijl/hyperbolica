use super::Rat;
use crate::core::PolyCtx;
use crate::error::Error;

#[test]
fn independent_integer_powers_match_generic_rational_powers() {
    let ctx = PolyCtx::new(["x", "y"]).unwrap();
    for expression in ["(2*x-y+1)/(3*x+y+2)", "(-2*x-y-1)/(5*y+3)", "-6/7", "0"] {
        let value = Rat::parse(ctx.clone(), expression).unwrap();
        for exponent in [-7_i64, -2, -1, 0, 1, 2, 7] {
            if value.is_zero() && exponent < 0 {
                assert!(matches!(value.pow(exponent), Err(Error::DivisionByZero)));
                continue;
            }
            let base = if exponent < 0 {
                value.native().clone().inv()
            } else {
                value.native().clone()
            };
            let expected =
                Rat::from_native(ctx.clone(), base.pow(exponent.unsigned_abs())).unwrap();
            let actual = value.pow(exponent).unwrap();
            assert_eq!(actual, expected, "({expression})^{exponent}");
            assert!(!actual.native().denominator.lcoeff().is_negative());
            assert!(!actual.compatibility_views_initialized());
        }
    }
}

#[test]
fn equal_denominator_arithmetic_removes_new_common_factors() {
    let ctx = PolyCtx::new(["x", "y"]).unwrap();
    let left = Rat::parse(ctx.clone(), "(x+1)/(x^2-y^2)").unwrap();
    let right = Rat::parse(ctx.clone(), "(y-1)/(x^2-y^2)").unwrap();
    assert_eq!(
        left.try_add(&right).unwrap(),
        Rat::parse(ctx.clone(), "1/(x-y)").unwrap()
    );
    let right = Rat::parse(ctx.clone(), "(-y+1)/(x^2-y^2)").unwrap();
    assert_eq!(
        left.try_sub(&right).unwrap(),
        Rat::parse(ctx.clone(), "1/(x-y)").unwrap()
    );
    assert!(left.try_sub(&left).unwrap().is_zero());
}

#[test]
fn derivative_over_constant_parameter_denominator_stays_reduced() {
    let ctx = PolyCtx::new(["x", "y"]).unwrap();
    for (expression, expected) in [
        ("(x*y+1)/y", "1"),
        ("(x*y^2+3*x^2*y+1)/(6*y^2)", "1/6+x/y"),
        ("(y+1)/(y-1)", "0"),
    ] {
        let value = Rat::parse(ctx.clone(), expression).unwrap();
        assert_eq!(
            value.derivative(0).unwrap(),
            Rat::parse(ctx.clone(), expected).unwrap()
        );
    }
}
