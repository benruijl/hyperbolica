use super::*;

#[test]
fn derivative_and_laurent_residue() {
    let ctx = context();
    let rational = Rat::parse(ctx.clone(), "(1+x)/(x^2*y)").unwrap();
    assert_eq!(rational.pole_degree(0).unwrap(), -2);
    assert_eq!(
        rational.residue(0).unwrap(),
        Rat::parse(ctx.clone(), "1/y").unwrap()
    );
    assert_eq!(
        rational.derivative(0).unwrap(),
        Rat::parse(ctx, "(-x-2)/(x^3*y)").unwrap()
    );
}

#[test]
fn laurent_leading_coefficients_stay_native_for_sparse_inputs_and_validate_zero() {
    let ctx = context();
    for (expression, variable, expected, order) in [
        ("x^40000*(y+1)/(x^3*(y^2-1))", 0, "1/(y-1)", 39997),
        ("(y^4*x+y^4+y^9)/(y^2*(x+2)+y^3)", 1, "(x+1)/(x+2)", 2),
        ("(2*x+4*y)/(6*y)", 0, "2/3", 0),
    ] {
        let value = Rat::parse(ctx.clone(), expression).unwrap();
        let residue = value.residue(variable).unwrap();
        assert_eq!(value.pole_degree(variable).unwrap(), order);
        assert_eq!(residue, Rat::parse(ctx.clone(), expected).unwrap());
        assert!(!value.compatibility_views_initialized());
        assert!(!residue.compatibility_views_initialized());
    }
    let zero = Rat::zero(ctx);
    assert!(matches!(zero.residue(2), Err(Error::UnknownVariable(_))));
    assert!(matches!(
        zero.pole_degree(2),
        Err(Error::UnknownVariable(_))
    ));
}

#[test]
fn absent_variable_and_scalar_composition_keep_exact_semantics() {
    let ctx = context();
    let value = Rat::parse(ctx.clone(), "(y^5+1)/(y^3+2)").unwrap();
    let replacement = Rat::parse(ctx.clone(), "1/(x+y)").unwrap();
    assert_eq!(value.substitute_rat(0, &replacement).unwrap(), value);
    assert_eq!(
        value.substitute_rational(0, &Rational::new(2, 3)).unwrap(),
        value
    );
    assert_eq!(
        value.substitute_integer(0, &Integer::from(7)).unwrap(),
        value
    );
    let scalar = Rat::from_rational(ctx.clone(), Rational::new(2, 3));
    assert_eq!(
        value.substitute_rat(1, &scalar).unwrap(),
        value.substitute_rational(1, &Rational::new(2, 3)).unwrap()
    );
    assert!(matches!(
        value.substitute_rat(2, &scalar),
        Err(Error::UnknownVariable(_))
    ));
    assert!(!value.compatibility_views_initialized());
}

#[test]
fn native_polynomial_part_integral_round_trips_without_q_views() {
    let ctx = context();
    let rational = Rat::parse(ctx.clone(), "(x^20+3*x^2+1)/(y+1)").unwrap();
    let primitive = rational.integrate_polynomial_part(0).unwrap();

    assert_eq!(primitive.derivative(0).unwrap(), rational);
    assert_eq!(
        primitive,
        Rat::parse(ctx.clone(), "(x^21/21+x^3+x)/(y+1)").unwrap()
    );
    assert!(rational.inner.views.get().is_none());
    assert!(primitive.inner.views.get().is_none());
    assert!(matches!(
        Rat::parse(ctx, "1/(x+1)")
            .unwrap()
            .integrate_polynomial_part(0),
        Err(Error::InvalidInput(_))
    ));
}
