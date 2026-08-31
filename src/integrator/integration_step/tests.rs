use super::{Boundary, IntegrationError, ShuffleEntry, integration_step};
use crate::core::{FactoredRat, PolyCtx, Rat};
use crate::reduce::MzvReductionTable;

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
fn factored_input_is_materialized_without_losing_the_denominator() {
    let ctx = PolyCtx::new(["x"]).unwrap();
    let mut entry = ShuffleEntry::new(Rat::one(ctx.clone()), Vec::new());
    entry.factored_den = Some(FactoredRat::parse(ctx.clone(), "1/(x+1)^2").unwrap());
    let result = integration_step(&ctx, &vec![entry], 0, &table(), false).unwrap();
    assert_eq!(result[0].coef.as_rat().unwrap(), Rat::one(ctx));
}
