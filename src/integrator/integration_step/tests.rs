use std::sync::Arc;

use symbolica::prelude::Symbol;

use super::contour::positive_integer;
use super::driver::StepTransformCache;
use super::{
    Boundary, IntegrationError, IntegrationStepOptions, ShuffleEntry, ShuffleEntrySym,
    integration_step, integration_step_sym_with_options_and_remaining_variables,
    integration_step_with_options_and_remaining_variables,
};
use crate::core::{FactoredRat, Poly, PolyCtx, Rat};
use crate::integrator::TransformResult;
use crate::reduce::MzvReductionTable;
use crate::symbols::Word;

fn table() -> MzvReductionTable {
    MzvReductionTable::default()
}

#[test]
fn convergent_double_pole_integrates_to_one() {
    let ctx = PolyCtx::new(["x"]).unwrap();
    let input = vec![ShuffleEntry::new(
        Rat::parse(ctx.clone(), "1/(x+1)^2").unwrap(),
        Vec::new(),
    )];
    let result = integration_step(&ctx, &input, 0, &table(), true).unwrap();
    assert_eq!(result.len(), 1);
    assert!(result[0].key.is_empty());
    assert_eq!(result[0].coef.as_rat().unwrap(), Rat::one(ctx));
}

#[test]
fn divergence_is_a_structured_error() {
    let ctx = PolyCtx::new(["x"]).unwrap();
    let input = vec![ShuffleEntry::new(
        Rat::parse(ctx.clone(), "1/(x+1)").unwrap(),
        Vec::new(),
    )];
    let error = integration_step(&ctx, &input, 0, &table(), true).unwrap_err();
    assert!(matches!(
        error,
        IntegrationError::Divergent {
            boundary: Boundary::Infinity,
            ..
        }
    ));
}

#[test]
fn rational_step_rejects_duplicate_and_current_remaining_variables() {
    let ctx = PolyCtx::new(["x", "y"]).unwrap();
    let input = Vec::<ShuffleEntry>::new();
    let options = IntegrationStepOptions::default();

    let duplicate = integration_step_with_options_and_remaining_variables(
        &ctx,
        &input,
        0,
        &table(),
        &options,
        &[1, 1],
    )
    .unwrap_err();
    assert!(duplicate.to_string().contains("listed more than once"));

    let current = integration_step_with_options_and_remaining_variables(
        &ctx,
        &input,
        0,
        &table(),
        &options,
        &[0],
    )
    .unwrap_err();
    assert!(current.to_string().contains("current integration variable"));
}

#[test]
fn symbolic_step_rejects_duplicate_and_current_remaining_variables() {
    let ctx = PolyCtx::new(["x", "y"]).unwrap();
    let input = Vec::<ShuffleEntrySym>::new();
    let options = IntegrationStepOptions::default();

    let duplicate = integration_step_sym_with_options_and_remaining_variables(
        &ctx,
        &input,
        0,
        &table(),
        &options,
        &[1, 1],
    )
    .unwrap_err();
    assert!(duplicate.to_string().contains("listed more than once"));

    let current = integration_step_sym_with_options_and_remaining_variables(
        &ctx,
        &input,
        0,
        &table(),
        &options,
        &[0],
    )
    .unwrap_err();
    assert!(current.to_string().contains("current integration variable"));
}

#[test]
fn factored_input_matches_materialized_step_for_multiple_denominator_blocks() {
    let ctx = PolyCtx::new(["x"]).unwrap();
    let mut factored = FactoredRat::from_poly(Poly::parse(ctx.clone(), "x^2+3*x+1").unwrap());
    factored
        .push_factor(&Poly::parse(ctx.clone(), "x+1").unwrap(), 3)
        .unwrap();
    factored
        .push_factor(&Poly::parse(ctx.clone(), "2*x+3").unwrap(), 2)
        .unwrap();
    factored
        .push_factor(&Poly::parse(ctx.clone(), "x+4").unwrap(), 2)
        .unwrap();

    let deferred = ShuffleEntry::from_factored(factored.clone(), Vec::new());
    assert!(deferred.coef.is_one());
    assert_eq!(
        deferred
            .factored_coefficient()
            .unwrap()
            .materialize()
            .unwrap(),
        factored.materialize().unwrap()
    );
    let materialized = ShuffleEntry::new(factored.materialize().unwrap(), Vec::new());
    let deferred_result = integration_step(&ctx, &vec![deferred], 0, &table(), false).unwrap();
    let materialized_result =
        integration_step(&ctx, &vec![materialized], 0, &table(), false).unwrap();
    assert_eq!(deferred_result, materialized_result);
}

#[test]
fn factored_input_rejects_a_changed_unit_sentinel() {
    let ctx = PolyCtx::new(["x"]).unwrap();
    let factored = FactoredRat::parse(ctx.clone(), "1/(x+1)^2").unwrap();
    let mut entry = ShuffleEntry::from_factored(factored, Vec::new());
    entry.coef = Rat::from_int(ctx.clone(), 2);

    let error = integration_step(&ctx, &vec![entry], 0, &table(), false).unwrap_err();
    assert!(error.to_string().contains("sentinel to remain one"));
}

#[test]
fn step_transform_cache_resolves_collision_by_full_shuffle_variable_and_namespace() {
    let left_symbol = Symbol::parse("x", "step_cache_left").unwrap();
    let right_symbol = Symbol::parse("x", "step_cache_right").unwrap();
    let left_ctx = PolyCtx::from_indeterminates([left_symbol.to_atom()]).unwrap();
    let right_ctx = PolyCtx::from_indeterminates([right_symbol.to_atom()]).unwrap();
    let left = vec![Word::from(vec![Rat::from_poly(
        Poly::generator(left_ctx, 0).unwrap(),
    )])];
    let right = vec![Word::from(vec![Rat::from_poly(
        Poly::generator(right_ctx, 0).unwrap(),
    )])];
    assert_eq!(left[0].to_string(), right[0].to_string());
    assert_ne!(left, right);

    let left_value = Arc::new(TransformResult::new());
    let right_value = Arc::new(TransformResult::new());
    let other_variable_value = Arc::new(TransformResult::new());
    let mut cache = StepTransformCache::default();
    cache.insert_in_bucket(&left, 0, left_value.clone(), 0);
    cache.insert_in_bucket(&right, 0, right_value.clone(), 0);
    cache.insert_in_bucket(&left, 1, other_variable_value.clone(), 0);

    assert!(Arc::ptr_eq(
        cache.get_in_bucket(&left, 0, 0).unwrap(),
        &left_value
    ));
    assert!(Arc::ptr_eq(
        cache.get_in_bucket(&right, 0, 0).unwrap(),
        &right_value
    ));
    assert!(Arc::ptr_eq(
        cache.get_in_bucket(&left, 1, 0).unwrap(),
        &other_variable_value
    ));
}

#[test]
fn positive_letter_detection_uses_exact_integer_constants() {
    let ctx = PolyCtx::new(["x"]).unwrap();
    assert_eq!(positive_integer(&Rat::from_int(ctx.clone(), 7)), Some(7));
    assert_eq!(positive_integer(&Rat::zero(ctx.clone())), None);
    assert_eq!(positive_integer(&Rat::from_int(ctx.clone(), -2)), None);
    assert_eq!(
        positive_integer(&Rat::parse(ctx.clone(), "3/2").unwrap()),
        None
    );
    assert_eq!(positive_integer(&Rat::parse(ctx, "x").unwrap()), None);
}
