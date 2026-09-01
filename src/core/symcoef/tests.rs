use std::sync::Arc;

use symbolica::prelude::Symbol;

use super::{SymCoef, SymMonomial, reduce_to_rat, simplify_symcoef};
use crate::core::{PolyCtx, Rat};
use crate::reduce::MzvReductionTable;
use crate::symbols::{SYMBOL_NAMESPACE, legacy, mzv_atom};

fn context() -> Arc<PolyCtx> {
    let x = Symbol::parse("x", SYMBOL_NAMESPACE).unwrap();
    PolyCtx::from_indeterminates([x.to_atom(), mzv_atom(&[2]), mzv_atom(&[3])]).unwrap()
}

fn rat(ctx: &Arc<PolyCtx>, expression: &str) -> Rat {
    let atom = legacy::parse_expression(expression).unwrap();
    Rat::from_atom(ctx.clone(), atom.as_view()).unwrap()
}

#[test]
fn canonicalizes_i_and_delta_powers() {
    let ctx = context();
    let mut inverse_i = SymMonomial::new(Rat::from_int(ctx.clone(), 3));
    inverse_i.i_power = -1;
    inverse_i.delta_powers.insert(0, -3);
    inverse_i.log_powers.insert(2, 0);

    let canonical = SymCoef::from_monomials(ctx.clone(), vec![inverse_i]);
    assert_eq!(canonical.terms()[0].i_power, 1);
    assert_eq!(canonical.terms()[0].delta_powers[&0], 1);
    assert!(canonical.terms()[0].log_powers.is_empty());
    assert_eq!(
        canonical.terms()[0].prefactor,
        Rat::from_int(ctx.clone(), -3)
    );

    let i = SymCoef::im_factor(ctx.clone());
    assert_eq!((&i * &i).as_rat().unwrap(), Rat::from_int(ctx.clone(), -1));
    assert_eq!(&(&i * &i) * &(&i * &i), SymCoef::one(ctx.clone()));

    let delta = SymCoef::delta_factor(ctx.clone(), 0).unwrap();
    assert_eq!((&delta * &delta).as_rat().unwrap(), Rat::one(ctx));
}

#[test]
fn canonical_collection_and_merge_add_sub() {
    let ctx = context();
    let mut first = SymMonomial::new(rat(&ctx, "mzv_3/x"));
    first.pi_power = 1;
    let mut second = SymMonomial::new(rat(&ctx, "6*mzv_2"));
    second.pi_power = 1;
    let combined = SymCoef::from_monomials(ctx.clone(), vec![second, first]);
    assert_eq!(combined.terms().len(), 1);
    assert_eq!(combined.terms()[0].prefactor, rat(&ctx, "mzv_3/x+6*mzv_2"));

    let pi = SymCoef::pi_factor(ctx.clone());
    let log = SymCoef::log_factor(ctx.clone(), 2).unwrap();
    let left = &combined + &log;
    let right = &pi + &log;
    let difference = left.try_sub(&right).unwrap();
    assert_eq!(difference.terms().len(), 1);
    assert_eq!(difference.terms()[0].pi_power, 1);
    assert_eq!(
        difference.terms()[0].prefactor,
        rat(&ctx, "mzv_3/x+6*mzv_2-1")
    );
}

#[test]
fn multiplication_is_a_cartesian_product() {
    let ctx = context();
    let pi = SymCoef::pi_factor(ctx.clone());
    let log = SymCoef::log_factor(ctx.clone(), 2).unwrap();
    let imaginary = SymCoef::im_factor(ctx.clone());
    let delta = SymCoef::delta_factor(ctx.clone(), 0).unwrap();

    let product = &(&pi + &log) * &(&imaginary + &delta);
    assert_eq!(product.terms().len(), 4);
    assert!(
        product
            .terms()
            .iter()
            .any(|term| term.pi_power == 1 && term.i_power == 1)
    );
    assert!(
        product.terms().iter().any(
            |term| term.log_powers.get(&2) == Some(&1) && term.delta_powers.get(&0) == Some(&1)
        )
    );
}

#[test]
fn formatting_is_deterministic_and_matches_hyperflint() {
    let x = Symbol::parse("x", SYMBOL_NAMESPACE).unwrap();
    let z = Symbol::parse("z", SYMBOL_NAMESPACE).unwrap();
    let ctx =
        PolyCtx::from_indeterminates([x.to_atom(), z.to_atom(), mzv_atom(&[2]), mzv_atom(&[3])])
            .unwrap();
    let mut symbolic = SymMonomial::new(rat(&ctx, "mzv_3/(x+1)"));
    symbolic.pi_power = 2;
    symbolic.i_power = 5;
    symbolic.log_powers.insert(7, 2);
    symbolic.log_powers.insert(2, 1);
    symbolic.delta_powers.insert(1, 1);
    symbolic.delta_powers.insert(0, 1);
    symbolic.period_powers.insert(12, 2);
    symbolic.period_powers.insert(3, 1);

    let coefficient = SymCoef::from_monomials(ctx, vec![symbolic]);
    let formatted = coefficient.to_string();
    assert!(formatted.contains("MZV(3)"));
    assert!(formatted.contains("*Pi^2*I*Log[2]*Log[7]^2"));
    assert!(formatted.contains("*delta[x]*delta[z]"));
    assert_eq!(
        coefficient.terms()[0].power_key(),
        "P2|I1|L2:1,7:2,|D0:1,1:1,|Q3:1,12:2,"
    );
}

#[test]
fn delta_identity_uses_context_indices_and_validates_bounds() {
    let left = Symbol::parse("x", "delta_identity_left").unwrap();
    let right = Symbol::parse("x", "delta_identity_right").unwrap();
    let ctx = PolyCtx::from_indeterminates([left.to_atom(), right.to_atom()]).unwrap();
    assert_eq!(ctx.vars(), &["x", "x"]);

    let left_delta = SymCoef::delta_factor(ctx.clone(), 0).unwrap();
    let right_delta = SymCoef::delta_factor(ctx.clone(), 1).unwrap();
    assert_ne!(left_delta, right_delta);
    assert!(left_delta.terms()[0].delta_powers.contains_key(&0));
    assert!(right_delta.terms()[0].delta_powers.contains_key(&1));
    assert!(matches!(
        SymCoef::delta_factor(ctx.clone(), ctx.len()),
        Err(crate::error::Error::UnknownVariable(_))
    ));

    let mut invalid = SymMonomial::new(Rat::one(ctx.clone()));
    invalid.delta_powers.insert(ctx.len(), 1);
    assert!(matches!(
        SymCoef::try_from_monomials(ctx, vec![invalid]),
        Err(crate::error::Error::UnknownVariable(_))
    ));
}

#[test]
fn rational_scaling_and_unwrap() {
    let ctx = context();
    let mzv = rat(&ctx, "mzv_3/(1+x)");
    let pure = SymCoef::from_rat(&mzv);
    assert!(pure.is_rat());
    assert_eq!(pure.as_rat().unwrap(), mzv);

    let scaled = SymCoef::pi_factor(ctx.clone())
        .try_mul_rat(&rat(&ctx, "6*mzv_2"))
        .unwrap();
    assert!(scaled.to_string().contains("MZV(2)"));
    assert!(!scaled.is_rat());
    assert!(scaled.as_rat().is_err());
    assert!(SymCoef::zero(ctx.clone()).is_rat());
    assert_eq!(SymCoef::zero(ctx.clone()).as_rat().unwrap(), Rat::zero(ctx));
}

#[test]
fn even_pi_powers_fold_into_mzv_two() {
    let ctx = context();
    let mut monomial = SymMonomial::new(rat(&ctx, "2*x"));
    monomial.pi_power = 4;
    let coefficient = SymCoef::from_monomials(ctx.clone(), vec![monomial]);
    let table = MzvReductionTable::default();
    let simplified = simplify_symcoef(&coefficient, &table).unwrap();
    assert_eq!(simplified.terms()[0].pi_power, 0);
    assert_eq!(simplified.as_rat().unwrap(), rat(&ctx, "72*x*mzv_2^2"));
    assert_eq!(
        reduce_to_rat(&coefficient, &table).unwrap(),
        rat(&ctx, "72*x*mzv_2^2")
    );
}

#[test]
fn reduction_refuses_residual_symbolic_generators() {
    let ctx = context();
    let odd_pi = SymCoef::pi_factor(ctx.clone());
    let logarithm = SymCoef::log_factor(ctx, 2).unwrap();
    let table = MzvReductionTable::default();
    assert!(reduce_to_rat(&odd_pi, &table).is_err());
    assert!(reduce_to_rat(&logarithm, &table).is_err());
}

#[test]
fn symbolic_coefficients_reject_equal_spellings_from_distinct_namespaces() {
    let left_symbol = Symbol::parse("x", "symcoef_context_left").unwrap();
    let right_symbol = Symbol::parse("x", "symcoef_context_right").unwrap();
    let left_ctx = PolyCtx::from_indeterminates([left_symbol.to_atom()]).unwrap();
    let right_ctx = PolyCtx::from_indeterminates([right_symbol.to_atom()]).unwrap();
    assert_eq!(left_ctx.vars(), right_ctx.vars());
    assert!(!left_ctx.is_compatible_with(&right_ctx));

    let left = SymCoef::one(left_ctx);
    let right = SymCoef::one(right_ctx);
    assert_ne!(left, right);
    assert!(matches!(
        left.try_add(&right),
        Err(crate::error::Error::ContextMismatch)
    ));
}

#[test]
fn symbolic_coefficients_accept_one_native_context_across_diagnostic_spellings() {
    let symbol = Symbol::parse("x", "symcoef_context_shared").unwrap();
    let qualified_ctx = PolyCtx::from_symbols([symbol]).unwrap();
    let stripped_ctx = PolyCtx::from_indeterminates([symbol.to_atom()]).unwrap();
    assert_ne!(qualified_ctx.vars(), stripped_ctx.vars());
    assert!(qualified_ctx.is_compatible_with(&stripped_ctx));

    let left = SymCoef::one(qualified_ctx);
    let right = SymCoef::one(stripped_ctx);
    assert_eq!(left, right);
    assert!(left.try_add(&right).is_ok());
}
