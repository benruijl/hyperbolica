use std::sync::Arc;

use symbolica::prelude::Symbol;

use super::{
    evaluate_periods, fibration_basis, fibration_basis_sym, test_zero_function_sym,
    zero_inf_period, zero_one_period,
};
use crate::core::{Poly, PolyCtx, Rat, SymCoef};
use crate::error::Error;
use crate::integrator::{RegKey, RegTerm, RegTermSym, Regulator};
use crate::reduce::{MzvReductionRule, MzvReductionTable, build_mzv_atom_list};
use crate::symbols::{SYMBOL_NAMESPACE, Word, mzv_atom};

fn setup() -> (Arc<PolyCtx>, MzvReductionTable) {
    let table = MzvReductionTable {
        reductions: vec![MzvReductionRule {
            lhs: "mzv_4".into(),
            rhs: "2/5*mzv_2^2".into(),
        }],
        basis: vec!["Log2".into(), "mzv_2".into(), "mzv_3".into()],
    };
    let x = Symbol::parse("x", SYMBOL_NAMESPACE).unwrap();
    let ctx =
        PolyCtx::from_indeterminates(build_mzv_atom_list(&table, [x.to_atom()]).unwrap()).unwrap();
    (ctx, table)
}

fn mzv(ctx: &Arc<PolyCtx>, indices: &[i64]) -> Rat {
    let atom = mzv_atom(indices);
    let index = ctx.index_of_indeterminate(atom.as_view()).unwrap();
    Rat::from_poly(Poly::generator(ctx.clone(), index).unwrap())
}

fn word(ctx: &Arc<PolyCtx>, letters: &[i64]) -> Word {
    Word::new(
        letters
            .iter()
            .map(|letter| Rat::from_int(ctx.clone(), *letter))
            .collect(),
    )
}

fn expression_word(ctx: &Arc<PolyCtx>, letters: &[&str]) -> Word {
    Word::new(
        letters
            .iter()
            .map(|letter| Rat::parse(ctx.clone(), letter).unwrap())
            .collect(),
    )
}

#[test]
fn zero_one_period_mints_and_reduces_mzvs() {
    let (ctx, table) = setup();
    assert_eq!(
        zero_one_period(&ctx, &word(&ctx, &[0, 1]), &table).unwrap(),
        mzv(&ctx, &[2]).negated()
    );
    assert_eq!(
        zero_one_period(&ctx, &word(&ctx, &[0, 0, 0, 1]), &table).unwrap(),
        mzv(&ctx, &[2])
            .pow(2)
            .unwrap()
            .try_mul(&Rat::from_int(ctx.clone(), -2))
            .unwrap()
            .try_div(&Rat::from_int(ctx.clone(), 5))
            .unwrap()
    );
}

#[test]
fn divergent_endpoint_words_are_regularized() {
    let (ctx, table) = setup();
    assert!(
        zero_one_period(&ctx, &word(&ctx, &[0, 0]), &table)
            .unwrap()
            .is_zero()
    );
    assert!(
        zero_inf_period(&ctx, &word(&ctx, &[-1, -1]), &table)
            .unwrap()
            .is_zero()
    );
}

#[test]
fn evaluate_periods_keeps_parametric_keys_and_folds_constants() {
    let (ctx, table) = setup();
    let regulator = vec![
        RegTerm {
            coef: Rat::from_int(ctx.clone(), 2),
            key: vec![word(&ctx, &[0, 1])],
        },
        RegTerm {
            coef: Rat::one(ctx.clone()),
            key: vec![Word::new(vec![Rat::parse(ctx.clone(), "x").unwrap()])],
        },
    ];
    let result = evaluate_periods(&ctx, &regulator, &table).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(
        result[0].coef,
        mzv(&ctx, &[2])
            .try_mul(&Rat::from_int(ctx.clone(), -2))
            .unwrap()
    );
    assert!(result[0].key.is_empty());
}

#[test]
fn fibration_basis_zero_variable_base_case_collects_constants() {
    let (ctx, table) = setup();
    let input = vec![
        RegTerm {
            coef: Rat::from_int(ctx.clone(), 7),
            key: RegKey::new(),
        },
        RegTerm {
            coef: Rat::from_int(ctx.clone(), -2),
            key: vec![Word::default()],
        },
    ];

    let result = fibration_basis(&ctx, &input, &[], &table).unwrap();
    assert!(result.vars.is_empty());
    assert_eq!(result.terms, vec![(RegKey::new(), Rat::from_int(ctx, 5))]);
}

#[test]
fn fibration_basis_transforms_requested_variable_and_aggregates() {
    let (ctx, table) = setup();
    let x = ctx.index_of("x").unwrap();
    let variable_word = expression_word(&ctx, &["-x"]);
    let first = vec![
        RegTerm {
            coef: Rat::from_int(ctx.clone(), 2),
            key: vec![variable_word.clone()],
        },
        RegTerm {
            coef: Rat::from_int(ctx.clone(), 3),
            key: vec![variable_word.clone()],
        },
    ];
    let mut reversed = first.clone();
    reversed.reverse();

    let result = fibration_basis(&ctx, &first, &[x], &table).unwrap();
    let reversed_result = fibration_basis(&ctx, &reversed, &[x], &table).unwrap();
    assert_eq!(result, reversed_result);
    assert_eq!(result.vars, vec!["x"]);
    assert_eq!(result.terms.len(), 1);
    assert_eq!(result.terms[0].0, vec![word(&ctx, &[0])]);
    assert_eq!(result.terms[0].1, Rat::from_int(ctx, -5));
}

#[test]
fn rational_fibration_rejects_an_unevaluable_terminal_period() {
    let (ctx, table) = setup();
    let input = vec![RegTerm {
        coef: Rat::one(ctx.clone()),
        key: vec![expression_word(&ctx, &["x"])],
    }];

    let error = fibration_basis(&ctx, &input, &[], &table).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("base-case period is not evaluable")
    );
}

#[test]
fn symbolic_fibration_keeps_unevaluable_periods_and_tests_per_key() {
    let (ctx, table) = setup();
    let key = vec![expression_word(&ctx, &["x"])];
    let pi = SymCoef::pi_factor(ctx.clone());
    let input = vec![RegTermSym {
        coef: pi.clone(),
        key: key.clone(),
    }];

    let result = fibration_basis_sym(&ctx, &input, &[], &table).unwrap();
    assert!(result.vars.is_empty());
    assert_eq!(result.terms, vec![(key.clone(), pi.clone())]);
    assert!(!test_zero_function_sym(&ctx, &input, &[], &table).unwrap());

    let cancelling = vec![
        RegTermSym {
            coef: pi.clone(),
            key: key.clone(),
        },
        RegTermSym {
            coef: pi.negated(),
            key,
        },
    ];
    assert!(test_zero_function_sym(&ctx, &cancelling, &[], &table).unwrap());
    assert!(
        fibration_basis_sym(&ctx, &cancelling, &[], &table)
            .unwrap()
            .terms
            .is_empty()
    );
}

#[test]
fn fibration_basis_validates_variable_indices_before_recursing() {
    let (ctx, table) = setup();
    let error = fibration_basis(&ctx, &Regulator::new(), &[ctx.len()], &table).unwrap_err();
    assert!(matches!(error, Error::UnknownVariable(_)));
}
