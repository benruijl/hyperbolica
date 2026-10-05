mod native_operations;

use std::cmp::Ordering;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use symbolica::prelude::{AtomCore, Integer, Rational, Symbol};

use super::Rat;
use crate::core::{Poly, PolyCtx};
use crate::error::Error;

fn context() -> Arc<PolyCtx> {
    PolyCtx::new(["x", "y"]).unwrap()
}

#[test]
fn parse_and_cancel() {
    let ctx = context();
    let rational = Rat::parse(ctx.clone(), "(x^2-y^2)/(x-y)").unwrap();
    assert_eq!(rational, Rat::parse(ctx, "x+y").unwrap());
}

#[test]
fn native_scalar_constructors_handle_signs_and_large_rationals() {
    let ctx = context();
    for value in [0, 1, -1, i64::MIN, i64::MAX] {
        let scalar = Rat::from_int(ctx.clone(), value);
        assert_eq!(scalar, Rat::parse(ctx.clone(), &value.to_string()).unwrap());
        assert!(!scalar.compatibility_views_initialized());
    }
    for expression in ["0", "-13/17", "18446744073709551616/3"] {
        let expected = Rat::parse(ctx.clone(), expression).unwrap();
        let scalar = Rat::from_rational(ctx.clone(), expected.rational_constant().unwrap());
        assert_eq!(scalar, expected);
        assert!(!scalar.compatibility_views_initialized());
    }
}

#[test]
fn arithmetic_identities_preserve_left_context_and_reject_foreign_contexts() {
    let symbol = Symbol::parse("identity_x", "rat_identity_context").unwrap();
    let left_ctx = PolyCtx::from_symbols([symbol]).unwrap();
    let right_ctx = PolyCtx::from_indeterminates([symbol.to_atom()]).unwrap();
    let left = Rat::from_poly(Poly::generator(left_ctx.clone(), 0).unwrap());
    let right = Rat::from_poly(Poly::generator(right_ctx.clone(), 0).unwrap());
    let zero = Rat::zero(left_ctx.clone());
    let one = Rat::one(left_ctx.clone());
    let right_zero = Rat::zero(right_ctx.clone());
    let right_one = Rat::one(right_ctx);

    for (actual, expected) in [
        (zero.try_add(&right).unwrap(), left.clone()),
        (left.try_add(&right_zero).unwrap(), left.clone()),
        (zero.try_sub(&right).unwrap(), left.negated()),
        (left.try_sub(&right_zero).unwrap(), left.clone()),
        (one.try_mul(&right).unwrap(), left.clone()),
        (left.try_mul(&right_one).unwrap(), left.clone()),
        (left.try_mul(&right_zero).unwrap(), zero.clone()),
        (zero.try_div(&right).unwrap(), zero.clone()),
        (left.try_div(&right_one).unwrap(), left.clone()),
    ] {
        assert_eq!(actual, expected);
        assert!(Arc::ptr_eq(actual.ctx(), &left_ctx));
        assert!(Arc::ptr_eq(actual.numerator().ctx(), &left_ctx));
    }

    let foreign = Rat::zero(PolyCtx::new(["unrelated_identity_y"]).unwrap());
    for error in [
        zero.try_add(&foreign),
        zero.try_sub(&foreign),
        zero.try_mul(&foreign),
        zero.try_div(&foreign),
    ] {
        assert!(matches!(error, Err(Error::ContextMismatch)));
    }
    assert!(matches!(
        zero.try_div(&right_zero),
        Err(Error::DivisionByZero)
    ));
}

#[test]
fn native_constructor_checks_both_polynomial_variable_maps() {
    let ctx = context();
    let source = Rat::parse(ctx.clone(), "x+y").unwrap();
    let foreign = Rat::one(PolyCtx::new(["x", "z"]).unwrap());
    let mixed = super::NativeRat {
        numerator: source.native().numerator.clone(),
        denominator: foreign.native().denominator.clone(),
    };
    assert!(matches!(
        Rat::from_native(ctx, mixed),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn equality_and_hash_use_native_variables_not_diagnostic_context_names() {
    let symbol = Symbol::parse("x", "rat_context_shared").unwrap();
    let qualified_ctx = PolyCtx::from_symbols([symbol]).unwrap();
    let stripped_ctx = PolyCtx::from_indeterminates([symbol.to_atom()]).unwrap();
    let qualified = Rat::from_poly(Poly::generator(qualified_ctx, 0).unwrap());
    let stripped = Rat::from_poly(Poly::generator(stripped_ctx, 0).unwrap());

    assert_ne!(qualified.ctx().vars(), stripped.ctx().vars());
    assert_eq!(qualified, stripped);
    let digest = |value: &Rat| {
        let mut state = DefaultHasher::new();
        value.hash(&mut state);
        state.finish()
    };
    assert_eq!(digest(&qualified), digest(&stripped));
    assert_eq!(qualified.structural_cmp(&stripped), Ordering::Equal);
}

#[test]
fn structural_order_is_total_across_values_and_namespaces() {
    let left_symbol = Symbol::parse("x", "rat_order_left").unwrap();
    let right_symbol = Symbol::parse("x", "rat_order_right").unwrap();
    let left_ctx = PolyCtx::from_symbols([left_symbol]).unwrap();
    let right_ctx = PolyCtx::from_symbols([right_symbol]).unwrap();
    let left = Rat::from_poly(Poly::generator(left_ctx.clone(), 0).unwrap());
    let right = Rat::from_poly(Poly::generator(right_ctx, 0).unwrap());
    assert_ne!(left, right);
    assert_ne!(left.structural_cmp(&right), Ordering::Equal);
    assert_eq!(
        left.structural_cmp(&right),
        right.structural_cmp(&left).reverse()
    );

    let one = Rat::one(left_ctx);
    let expanded = left
        .pow(2)
        .unwrap()
        .try_sub(&one)
        .unwrap()
        .try_div(&left.try_sub(&one).unwrap())
        .unwrap();
    let canonical = left.try_add(&one).unwrap();
    assert_eq!(expanded, canonical);
    assert_eq!(expanded.structural_cmp(&canonical), Ordering::Equal);
}

#[test]
fn arithmetic_stays_canonical() {
    let ctx = context();
    let a = Rat::parse(ctx.clone(), "1/x").unwrap();
    let b = Rat::parse(ctx.clone(), "1/y").unwrap();
    assert_eq!(&a + &b, Rat::parse(ctx.clone(), "(x+y)/(x*y)").unwrap());
    assert_eq!(&a * &b, Rat::parse(ctx, "1/(x*y)").unwrap());
}

#[test]
fn native_value_is_shared_and_q_views_are_lazy_and_monic() {
    let ctx = context();
    let rational = Rat::parse(ctx.clone(), "(x+1)/(2*y+2)").unwrap();
    let cloned = rational.clone();

    assert!(std::ptr::eq(rational.native(), cloned.native()));
    assert!(std::ptr::eq(&rational.inner.views, &cloned.inner.views));
    assert!(rational.inner.views.get().is_none());

    assert_eq!(
        rational.numerator(),
        &Poly::parse(ctx.clone(), "x/2+1/2").unwrap()
    );
    assert_eq!(rational.denominator(), &Poly::parse(ctx, "y+1").unwrap());
    assert!(cloned.inner.views.get().is_some());
    let warm_clone = rational.clone();
    assert!(std::ptr::eq(cloned.numerator(), warm_clone.numerator()));
    assert!(std::ptr::eq(cloned.denominator(), warm_clone.denominator()));
}

#[test]
fn concurrent_clones_preserve_context_specific_lazy_views() {
    let symbol = Symbol::parse("x", "rat_concurrent_views").unwrap();
    let qualified = PolyCtx::from_symbols([symbol]).unwrap();
    let stripped = PolyCtx::from_indeterminates([symbol.to_atom()]).unwrap();
    assert_ne!(qualified.vars(), stripped.vars());
    let original = Rat::from_poly(Poly::generator(qualified.clone(), 0).unwrap())
        .try_div(&Rat::from_int(qualified.clone(), 2))
        .unwrap();
    let rebound = original.clone_in_context(&stripped);
    let values = (0..8)
        .map(|i| {
            if i % 2 == 0 {
                (original.clone(), qualified.clone())
            } else {
                (rebound.clone(), stripped.clone())
            }
        })
        .collect::<Vec<_>>();
    drop(original);
    drop(rebound);
    let barrier = std::sync::Barrier::new(values.len());
    std::thread::scope(|scope| {
        for (value, expected_ctx) in values {
            let barrier = &barrier;
            scope.spawn(move || {
                barrier.wait();
                for _ in 0..32 {
                    let cloned = value.clone();
                    assert!(Arc::ptr_eq(cloned.ctx(), &expected_ctx));
                    assert!(Arc::ptr_eq(cloned.numerator().ctx(), &expected_ctx));
                    assert!(Arc::ptr_eq(cloned.denominator().ctx(), &expected_ctx));
                    assert_eq!(
                        cloned.evaluate_rational(&[Rational::from(4)]).unwrap(),
                        Rational::from(2)
                    );
                }
            });
        }
    });
}

#[test]
fn context_aliases_share_native_storage_without_retaining_intermediate_contexts() {
    let symbol = Symbol::parse("x", "rat_context_aliases").unwrap();
    let root_ctx = PolyCtx::from_symbols([symbol]).unwrap();
    let middle_ctx = PolyCtx::from_indeterminates([symbol.to_atom()]).unwrap();
    let last_ctx = PolyCtx::from_symbols([symbol]).unwrap();
    let original = Rat::from_poly(Poly::generator(root_ctx, 0).unwrap());
    let middle = original.clone_in_context(&middle_ctx);
    let intermediate = Arc::downgrade(&middle_ctx);
    // Warm the intermediate views too: later aliases must retain neither the
    // intermediate wrapper nor the context held by its cached polynomials.
    let _ = middle.numerator();
    let last = middle.clone_in_context(&last_ctx);
    assert!(std::ptr::eq(original.native(), middle.native()));
    assert!(std::ptr::eq(original.native(), last.native()));
    drop(middle);
    drop(middle_ctx);
    assert!(intermediate.upgrade().is_none());
    drop(original);
    assert!(Arc::ptr_eq(last.ctx(), &last_ctx));
    assert!(Arc::ptr_eq(last.numerator().ctx(), &last_ctx));
    assert_eq!(
        last.evaluate_rational(&[Rational::from(7)]).unwrap(),
        Rational::from(7)
    );
}

#[test]
fn exact_constant_extraction_stays_on_the_native_symbolica_value() {
    let ctx = context();
    let rational = Rat::parse(ctx.clone(), "-42/35").unwrap();
    let integer = Rat::parse(ctx.clone(), "-17").unwrap();
    let nonconstant = Rat::parse(ctx, "(x+1)/(y+1)").unwrap();

    assert_eq!(rational.rational_constant(), Some(Rational::new(-6, 5)));
    assert_eq!(rational.integer_constant(), None);
    assert_eq!(integer.integer_constant(), Some(Integer::from(-17)));
    assert_eq!(nonconstant.rational_constant(), None);

    assert!(rational.inner.views.get().is_none());
    assert!(integer.inner.views.get().is_none());
    assert!(nonconstant.inner.views.get().is_none());
}

#[test]
fn native_degree_dependency_and_order_queries_keep_q_views_cold() {
    let ctx = context();
    let rational = Rat::parse(ctx.clone(), "(x^3+x*y+1)/(x*y^2+y+1)").unwrap();
    let zero = Rat::zero(ctx);

    assert_eq!(rational.numerator_degree(0).unwrap(), 3);
    assert_eq!(rational.numerator_degree(1).unwrap(), 1);
    assert_eq!(rational.denominator_degree(0).unwrap(), 1);
    assert_eq!(rational.denominator_degree(1).unwrap(), 2);
    assert!(rational.depends_on(0).unwrap());
    assert!(rational.depends_on(1).unwrap());
    assert_eq!(rational.pole_degree(0).unwrap(), 0);

    assert_eq!(zero.numerator_degree(0).unwrap(), -1);
    assert_eq!(zero.denominator_degree(0).unwrap(), 0);
    assert!(!zero.depends_on(0).unwrap());
    assert!(matches!(
        rational.depends_on(2),
        Err(Error::UnknownVariable(_))
    ));
    assert!(matches!(
        rational.numerator_degree(2),
        Err(Error::UnknownVariable(_))
    ));
    assert!(matches!(
        rational.denominator_degree(2),
        Err(Error::UnknownVariable(_))
    ));

    assert!(rational.inner.views.get().is_none());
    assert!(zero.inner.views.get().is_none());
}

#[test]
fn constructor_delegates_content_and_polynomial_cancellation_to_symbolica() {
    let ctx = context();
    let numerator = Poly::parse(ctx.clone(), "3/10*(x^2-y^2)").unwrap();
    let denominator = Poly::parse(ctx.clone(), "9/14*(x-y)").unwrap();
    let rational = Rat::new(numerator, denominator).unwrap();

    assert_eq!(rational, Rat::parse(ctx.clone(), "7/15*(x+y)").unwrap());
    assert!(rational.inner.native.denominator.is_constant());
    assert!(!rational.inner.native.denominator.lcoeff().is_negative());
    assert_eq!(
        Rat::from_atom(ctx, rational.to_atom().as_view()).unwrap(),
        rational
    );
}

#[test]
fn constructor_removes_scalar_content_from_disjoint_variables() {
    let ctx = context();
    let numerator = Poly::parse(ctx.clone(), "21+63*x^2+21*x^20").unwrap();
    let denominator = Poly::parse(ctx.clone(), "21+21*y").unwrap();

    assert_eq!(
        Rat::new(numerator, denominator).unwrap(),
        Rat::parse(ctx, "(1+3*x^2+x^20)/(1+y)").unwrap()
    );
}

#[test]
fn every_arithmetic_kernel_matches_exact_symbolica_normalization() {
    let ctx = context();
    let left = Rat::parse(ctx.clone(), "(x^3+2*x*y-y)/(x^2-y^2)").unwrap();
    let right = Rat::parse(ctx.clone(), "(x-y+3)/(x*y+y^2)").unwrap();

    assert_eq!(
        left.try_add(&right).unwrap(),
        Rat::from_atom(ctx.clone(), (left.to_atom() + right.to_atom()).as_view()).unwrap()
    );
    assert_eq!(
        left.try_mul(&right).unwrap(),
        Rat::from_atom(ctx.clone(), (left.to_atom() * right.to_atom()).as_view()).unwrap()
    );
    assert_eq!(
        left.try_div(&right).unwrap(),
        Rat::from_atom(ctx.clone(), (left.to_atom() / right.to_atom()).as_view()).unwrap()
    );
    assert_eq!(
        left.try_sub(&right).unwrap(),
        Rat::from_atom(ctx.clone(), (left.to_atom() - right.to_atom()).as_view()).unwrap()
    );
    assert_eq!(
        left.pow(13).unwrap(),
        Rat::from_atom(ctx, left.to_atom().pow(13).as_view()).unwrap()
    );
}

#[test]
fn large_shared_denominator_addition_cancels_once_and_stays_canonical() {
    let ctx = PolyCtx::new(["x", "y", "z"]).unwrap();
    let common = "(x+y+1)^8*(z+x+2)^6";
    let left = Rat::parse(ctx.clone(), &format!("(x^7+y^5*z+3*x*y+1)/({common})")).unwrap();
    let right = Rat::parse(ctx.clone(), &format!("(y^7-x^4*z+2*x*z+5)/({common})")).unwrap();
    let sum = left.try_add(&right).unwrap();
    let expected = Rat::parse(
        ctx,
        &format!("(x^7+y^7+y^5*z-x^4*z+3*x*y+2*x*z+6)/({common})"),
    )
    .unwrap();

    assert_eq!(sum, expected);
    assert!(!sum.inner.native.denominator.lcoeff().is_negative());
}

#[test]
fn context_and_domain_errors_remain_typed() {
    let xy = context();
    let yx = PolyCtx::new(["y", "x"]).unwrap();
    let left = Rat::parse(xy.clone(), "x/y").unwrap();
    let reordered = Rat::parse(yx, "x/y").unwrap();

    assert!(matches!(
        left.try_add(&reordered),
        Err(Error::ContextMismatch)
    ));
    assert!(matches!(
        Rat::new(Poly::one(xy.clone()), Poly::zero(xy.clone())),
        Err(Error::DivisionByZero)
    ));
    assert!(matches!(
        left.try_div(&Rat::zero(xy.clone())),
        Err(Error::DivisionByZero)
    ));
    assert!(matches!(
        Rat::zero(xy.clone()).pow(-1),
        Err(Error::DivisionByZero)
    ));
    assert!(matches!(
        left.pow(i64::MIN),
        Err(Error::InvalidExponent(i64::MIN))
    ));
    assert!(matches!(left.derivative(2), Err(Error::UnknownVariable(_))));
    assert!(Rat::parse(xy, "z").is_err());
}

#[test]
fn negative_power_and_typed_native_substitution_are_exact() {
    let ctx = context();
    let rational = Rat::parse(ctx.clone(), "(x+2*y)/(x-y)").unwrap();
    assert_eq!(
        rational.pow(-3).unwrap(),
        Rat::parse(ctx.clone(), "(x-y)^3/(x+2*y)^3").unwrap()
    );
    let substituted = rational
        .substitute_rational(0, &Rational::new(1, 2))
        .unwrap();
    assert_eq!(
        substituted,
        Rat::parse(ctx.clone(), "(1+4*y)/(1-2*y)").unwrap()
    );
    assert_eq!(
        rational.substitute_integer(0, &Integer::from(2)).unwrap(),
        Rat::parse(ctx.clone(), "(2+2*y)/(2-y)").unwrap()
    );
    assert!(rational.inner.views.get().is_none());
    assert!(substituted.inner.views.get().is_none());
}

#[test]
fn rational_function_substitution_uses_native_horner_composition() {
    let ctx = context();
    let rational = Rat::parse(ctx.clone(), "(x^17+2*x^3*y+y^2+1)/(x^5-x^2*y+y+2)").unwrap();
    let replacement = Rat::parse(ctx.clone(), "(y^2-1)/(y+2)").unwrap();
    let substituted = rational.substitute_rat(0, &replacement).unwrap();
    let expected = Rat::parse(
        ctx,
        "(((y^2-1)/(y+2))^17+2*((y^2-1)/(y+2))^3*y+y^2+1)/(((y^2-1)/(y+2))^5-((y^2-1)/(y+2))^2*y+y+2)",
    )
    .unwrap();

    assert_eq!(substituted, expected);
    assert!(rational.inner.views.get().is_none());
    assert!(replacement.inner.views.get().is_none());
    assert!(substituted.inner.views.get().is_none());
}

#[test]
fn rational_function_substitution_preserves_typed_failures() {
    let ctx = context();
    let rational = Rat::parse(ctx.clone(), "(x+y)/(x-1)").unwrap();
    let one = Rat::one(ctx.clone());
    let foreign = Rat::one(PolyCtx::new(["z"]).unwrap());

    assert!(matches!(
        rational.substitute_rat(0, &one),
        Err(Error::DivisionByZero)
    ));
    assert!(matches!(
        rational.substitute_rat(2, &one),
        Err(Error::UnknownVariable(_))
    ));
    assert!(matches!(
        rational.substitute_rat(0, &foreign),
        Err(Error::ContextMismatch)
    ));
}

#[test]
fn native_context_transfer_keeps_compatibility_views_cold() {
    let source_ctx = PolyCtx::new(["x", "y"]).unwrap();
    let destination_ctx = PolyCtx::new(["z", "y", "x"]).unwrap();
    let source = Rat::parse(source_ctx, "(x+y)/(1-x*y)").unwrap();
    let transferred =
        crate::reduce::cross_ctx_transfer_rat(&source, destination_ctx.clone()).unwrap();

    assert_eq!(
        transferred,
        Rat::parse(destination_ctx, "(x+y)/(1-x*y)").unwrap()
    );
    assert!(source.inner.views.get().is_none());
    assert!(transferred.inner.views.get().is_none());
}

#[test]
fn rational_and_integer_evaluation_are_exact_and_check_poles() {
    let ctx = context();
    let rational = Rat::parse(ctx.clone(), "(x+2*y)/(x-y)").unwrap();
    assert_eq!(
        rational
            .evaluate_rational(&[Rational::new(1, 2), Rational::new(1, 3)])
            .unwrap(),
        Rational::from(7)
    );
    assert_eq!(
        rational
            .evaluate_integer(&[Integer::from(2), Integer::from(1)])
            .unwrap(),
        Rational::from(4)
    );
    assert!(matches!(
        rational.evaluate_rational(&[Rational::one(), Rational::one()]),
        Err(Error::DivisionByZero)
    ));
    assert!(matches!(
        rational.evaluate_integer(&[Integer::from(1)]),
        Err(Error::InvalidInput(_))
    ));
    assert!(rational.inner.views.get().is_none());

    let zero = Rat::zero(ctx);
    assert_eq!(
        zero.evaluate_integer(&[Integer::from(13), Integer::from(-8)])
            .unwrap(),
        Rational::zero()
    );
    assert!(
        zero.substitute_rational(1, &Rational::new(5, 9))
            .unwrap()
            .is_zero()
    );
    assert!(zero.inner.views.get().is_none());
}

#[test]
fn native_substitution_detects_zero_denominators_without_materializing_views() {
    let ctx = context();
    let rational = Rat::parse(ctx, "(x+y)/(x-1)").unwrap();
    assert!(matches!(
        rational.substitute_rational(0, &Rational::one()),
        Err(Error::DivisionByZero)
    ));
    assert!(matches!(
        rational.substitute_integer(0, &Integer::from(1)),
        Err(Error::DivisionByZero)
    ));
    assert!(matches!(
        rational.substitute_integer(2, &Integer::from(1)),
        Err(Error::UnknownVariable(_))
    ));
    assert!(rational.inner.views.get().is_none());
}

#[test]
fn sparse_substitution_commutes_with_full_evaluation() {
    let ctx = context();
    let expressions = [
        "(x^17+2*y^9+1)/(x^2+y^2+1)",
        "(3*x*y-2)/(x+y+5)",
        "(x^31-y^13)/(2*x^2+3*y^2+1)",
    ];
    let points = [
        (Rational::new(2, 3), Rational::new(-1, 2)),
        (Rational::from(3), Rational::from(2)),
    ];

    for expression in expressions {
        let rational = Rat::parse(ctx.clone(), expression).unwrap();
        for (x, y) in &points {
            let direct = rational.evaluate_rational(&[x.clone(), y.clone()]).unwrap();
            let after_substitution = rational
                .substitute_rational(0, x)
                .unwrap()
                .evaluate_rational(&[Rational::zero(), y.clone()])
                .unwrap();
            assert_eq!(after_substitution, direct, "{expression} at ({x}, {y})");
        }
    }
}
