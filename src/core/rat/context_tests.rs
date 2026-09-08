//! Native arithmetic can reorder or omit context variables without changing
//! their identities. The port boundary restores its own ordered context.

use std::sync::Arc;

use symbolica::prelude::*;

use super::{NativeRat, Rat};
use crate::core::{Poly, PolyCtx};
use crate::error::Error;

fn assert_in_context(actual: &Rat, ctx: &Arc<PolyCtx>, expected: Rat) {
    assert_eq!(actual, &expected);
    assert!(Arc::ptr_eq(actual.ctx(), ctx));
    assert_eq!(actual.native().numerator.variables(), &ctx.variable_map());
    assert_eq!(actual.native().denominator.variables(), &ctx.variable_map());
    assert!(!actual.compatibility_views_initialized());
}

#[test]
fn native_permutation_restores_order_and_leading_denominator_sign() {
    let source_ctx = PolyCtx::new(["x", "y"]).unwrap();
    let target_ctx = PolyCtx::new(["y", "x"]).unwrap();
    let source = Rat::parse(source_ctx, "(x+2)/(x-y)").unwrap();
    let native = source.native().clone();
    let actual = Rat::from_native(target_ctx.clone(), native).unwrap();
    assert_in_context(
        &actual,
        &target_ctx,
        Rat::parse(target_ctx.clone(), "(x+2)/(x-y)").unwrap(),
    );
    assert!(!actual.native().denominator.lcoeff().is_negative());
}

#[test]
fn independent_native_subsets_grow_into_the_requested_context() {
    let target_ctx = PolyCtx::new(["unused", "y", "x"]).unwrap();
    let numerator = Rat::parse(PolyCtx::new(["x"]).unwrap(), "x+1").unwrap();
    let denominator = Rat::parse(PolyCtx::new(["y"]).unwrap(), "y+2").unwrap();
    let native = NativeRat {
        numerator: numerator.native().numerator.clone(),
        denominator: denominator.native().numerator.clone(),
    };
    let actual = Rat::from_native(target_ctx.clone(), native).unwrap();
    assert_in_context(
        &actual,
        &target_ctx,
        Rat::parse(target_ctx.clone(), "(x+1)/(y+2)").unwrap(),
    );
}

#[test]
fn variable_free_constants_and_zero_grow_without_warming_views() {
    let empty = PolyCtx::new(Vec::<String>::new()).unwrap();
    let target_ctx = PolyCtx::new(["y", "x"]).unwrap();
    for expression in ["0", "1", "-7/3"] {
        let source = Rat::parse(empty.clone(), expression).unwrap();
        let actual = Rat::from_native(target_ctx.clone(), source.native().clone()).unwrap();
        assert_in_context(
            &actual,
            &target_ctx,
            Rat::parse(target_ctx.clone(), expression).unwrap(),
        );
        let unchanged = Rat::from_native(empty.clone(), source.native().clone()).unwrap();
        assert_in_context(&unchanged, &empty, source);
    }
}

#[test]
fn remapping_preserves_namespaces_and_function_indeterminates() {
    let left = Symbol::parse("x", "rat_remap_left").unwrap();
    let right = Symbol::parse("x", "rat_remap_right").unwrap();
    let function = Atom::parse("f(x)", "rat_remap_function", ParseSettings::default()).unwrap();
    let target_ctx =
        PolyCtx::from_indeterminates([right.to_atom(), function.clone(), left.to_atom()]).unwrap();
    let source_ctx = PolyCtx::from_indeterminates([left.to_atom(), function]).unwrap();
    for (source_index, target_index) in [(0, 2), (1, 1)] {
        let source = Rat::from_poly(Poly::generator(source_ctx.clone(), source_index).unwrap());
        let actual = Rat::from_native(target_ctx.clone(), source.native().clone()).unwrap();
        assert_in_context(
            &actual,
            &target_ctx,
            Rat::from_poly(Poly::generator(target_ctx.clone(), target_index).unwrap()),
        );
    }
}

#[test]
fn unknown_active_and_unused_declarations_are_rejected_in_both_maps() {
    let target_ctx = PolyCtx::new(["x", "y"]).unwrap();
    let known = Rat::one(target_ctx.clone());
    let foreign_ctx = PolyCtx::new(["x", "foreign"]).unwrap();
    for expression in ["1", "foreign+1"] {
        let foreign = Rat::parse(foreign_ctx.clone(), expression).unwrap();
        for numerator_is_foreign in [false, true] {
            let (numerator, denominator) = if numerator_is_foreign {
                (&foreign.native().numerator, &known.native().denominator)
            } else {
                (&known.native().numerator, &foreign.native().numerator)
            };
            assert!(matches!(
                Rat::from_native(
                    target_ctx.clone(),
                    NativeRat {
                        numerator: numerator.clone(),
                        denominator: denominator.clone(),
                    }
                ),
                Err(Error::InvalidInput(_))
            ));
        }
    }

    let foreign = Symbol::parse("x", "rat_remap_unknown_namespace").unwrap();
    let foreign = Rat::one(PolyCtx::from_symbols([foreign]).unwrap());
    assert!(matches!(
        Rat::from_native(target_ctx, foreign.native().clone()),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn zero_denominator_is_still_rejected_before_context_remapping() {
    let target_ctx = PolyCtx::new(["x", "y"]).unwrap();
    let source = Rat::one(PolyCtx::new(["x"]).unwrap());
    let native = NativeRat {
        numerator: source.native().numerator.clone(),
        denominator: source.native().denominator.zero(),
    };
    assert!(matches!(
        Rat::from_native(target_ctx, native),
        Err(Error::DivisionByZero)
    ));
}
