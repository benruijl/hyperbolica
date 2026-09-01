use std::sync::Arc;

use symbolica::prelude::Symbol;

use super::conversion::integer_letter;
use super::fibration::{FibBasisAcc, FibBasisAccSym};
use super::{
    evaluate_periods, fibration_basis, fibration_basis_sym, test_zero_function_sym,
    zero_inf_period, zero_one_period,
};
use crate::core::{Poly, PolyCtx, Rat, SymCoef};
use crate::error::Error;
use crate::integrator::{RegKey, RegTerm, RegTermSym, Regulator, canonicalize_regkey};
use crate::reduce::{
    MzvReductionRule, MzvReductionTable, build_mzv_atom_list, build_mzv_basis_atom_list,
    standard_mzv_reductions,
};
use crate::symbols::{SYMBOL_NAMESPACE, Word, mzv_atom};

fn setup() -> (Arc<PolyCtx>, MzvReductionTable) {
    let table = MzvReductionTable::from_parts(
        vec![MzvReductionRule {
            lhs: "mzv_4".into(),
            rhs: "2/5*mzv_2^2".into(),
        }],
        vec!["Log2".into(), "mzv_2".into(), "mzv_3".into()],
    );
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
fn embedded_standard_expansion_reduces_a_nonbasis_period_in_a_narrow_context() {
    let table = standard_mzv_reductions();
    let ctx = PolyCtx::from_indeterminates(build_mzv_basis_atom_list(&table, []).unwrap()).unwrap();
    assert!(
        ctx.index_of_indeterminate(mzv_atom(&[4]).as_view())
            .is_none()
    );

    let actual = zero_one_period(&ctx, &word(&ctx, &[0, 0, 0, 1]), &table).unwrap();
    let expected = mzv(&ctx, &[2])
        .pow(2)
        .unwrap()
        .try_mul(&Rat::from_int(ctx.clone(), -2))
        .unwrap()
        .try_div(&Rat::from_int(ctx.clone(), 5))
        .unwrap();
    assert_eq!(actual, expected);

    let empty = MzvReductionTable::default();
    let empty_ctx =
        PolyCtx::from_indeterminates(build_mzv_basis_atom_list(&empty, []).unwrap()).unwrap();
    assert!(zero_one_period(&empty_ctx, &word(&empty_ctx, &[0, 0, 0, 1]), &empty).is_err());
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
            key: vec![word(&ctx, &[0, -1])],
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
            .try_mul(&Rat::from_int(ctx.clone(), 2))
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

#[test]
fn fibration_accumulators_resolve_forced_collisions_structurally() {
    let (ctx, _) = setup();
    let first_key = vec![word(&ctx, &[1])];
    let second_key = vec![word(&ctx, &[2])];
    let forced_digest = 0;

    let mut rational = FibBasisAcc::default();
    rational
        .add_with_digest(
            first_key.clone(),
            Rat::from_int(ctx.clone(), 2),
            forced_digest,
        )
        .unwrap();
    rational
        .add_with_digest(
            second_key.clone(),
            Rat::from_int(ctx.clone(), 3),
            forced_digest,
        )
        .unwrap();
    rational
        .add_with_digest(
            first_key.clone(),
            Rat::from_int(ctx.clone(), 5),
            forced_digest,
        )
        .unwrap();
    let rational_terms = rational.into_terms();
    assert_eq!(rational_terms.len(), 2);
    assert_eq!(
        rational_terms[0],
        (first_key.clone(), Rat::from_int(ctx.clone(), 7))
    );
    assert_eq!(
        rational_terms[1],
        (second_key.clone(), Rat::from_int(ctx.clone(), 3))
    );

    let mut symbolic = FibBasisAccSym::default();
    symbolic
        .add_with_digest(
            first_key.clone(),
            SymCoef::from_rat(&Rat::from_int(ctx.clone(), 2)),
            forced_digest,
        )
        .unwrap();
    symbolic
        .add_with_digest(
            second_key.clone(),
            SymCoef::from_rat(&Rat::from_int(ctx.clone(), 3)),
            forced_digest,
        )
        .unwrap();
    symbolic
        .add_with_digest(
            first_key,
            SymCoef::from_rat(&Rat::from_int(ctx.clone(), 5)),
            forced_digest,
        )
        .unwrap();
    let symbolic_terms = symbolic.into_terms();
    assert_eq!(symbolic_terms.len(), 2);
    assert_eq!(
        symbolic_terms[0].1,
        SymCoef::from_rat(&Rat::from_int(ctx.clone(), 7))
    );
    assert_eq!(
        symbolic_terms[1].1,
        SymCoef::from_rat(&Rat::from_int(ctx, 3))
    );
}

#[test]
fn rational_fibration_accumulator_merges_and_cancels_reversed_products() {
    let (ctx, _) = setup();
    let forward = vec![word(&ctx, &[2]), word(&ctx, &[10])];
    let mut reversed = forward.clone();
    reversed.reverse();
    let canonical = canonicalize_regkey(&forward);

    let mut merged = FibBasisAcc::default();
    merged
        .add(forward.clone(), Rat::from_int(ctx.clone(), 2))
        .unwrap();
    merged
        .add(reversed.clone(), Rat::from_int(ctx.clone(), 3))
        .unwrap();
    assert_eq!(
        merged.into_terms(),
        vec![(canonical, Rat::from_int(ctx.clone(), 5))]
    );

    let mut cancelled = FibBasisAcc::default();
    cancelled
        .add_with_digest(forward, Rat::from_int(ctx.clone(), 7), 0)
        .unwrap();
    cancelled
        .add_with_digest(reversed, Rat::from_int(ctx, -7), 0)
        .unwrap();
    assert!(cancelled.into_terms().is_empty());
}

#[test]
fn symbolic_fibration_accumulator_merges_and_cancels_reversed_products() {
    let (ctx, _) = setup();
    let forward = vec![word(&ctx, &[2]), word(&ctx, &[10])];
    let mut reversed = forward.clone();
    reversed.reverse();
    let canonical = canonicalize_regkey(&forward);

    let mut merged = FibBasisAccSym::default();
    merged
        .add(
            forward.clone(),
            SymCoef::from_rat(&Rat::from_int(ctx.clone(), 2)),
        )
        .unwrap();
    merged
        .add(
            reversed.clone(),
            SymCoef::from_rat(&Rat::from_int(ctx.clone(), 3)),
        )
        .unwrap();
    assert_eq!(
        merged.into_terms(),
        vec![(canonical, SymCoef::from_rat(&Rat::from_int(ctx.clone(), 5)))]
    );

    let mut cancelled = FibBasisAccSym::default();
    cancelled
        .add_with_digest(
            forward,
            SymCoef::from_rat(&Rat::from_int(ctx.clone(), 7)),
            0,
        )
        .unwrap();
    cancelled
        .add_with_digest(reversed, SymCoef::from_rat(&Rat::from_int(ctx, -7)), 0)
        .unwrap();
    assert!(cancelled.into_terms().is_empty());
}

#[test]
fn fibration_accumulators_reuse_cancelled_slots() {
    let (ctx, _) = setup();
    let forward = vec![word(&ctx, &[2]), word(&ctx, &[10])];
    let mut reversed = forward.clone();
    reversed.reverse();
    let forced_digest = 0;

    let mut rational = FibBasisAcc::default();
    let mut symbolic = FibBasisAccSym::default();
    for _ in 0..64 {
        rational
            .add_with_digest(forward.clone(), Rat::one(ctx.clone()), forced_digest)
            .unwrap();
        rational
            .add_with_digest(
                reversed.clone(),
                Rat::from_int(ctx.clone(), -1),
                forced_digest,
            )
            .unwrap();
        symbolic
            .add_with_digest(forward.clone(), SymCoef::one(ctx.clone()), forced_digest)
            .unwrap();
        symbolic
            .add_with_digest(
                reversed.clone(),
                SymCoef::from_rat(&Rat::from_int(ctx.clone(), -1)),
                forced_digest,
            )
            .unwrap();
    }

    assert_eq!(rational.storage_shape(forced_digest), (1, 1));
    assert_eq!(symbolic.storage_shape(forced_digest), (1, 1));
    assert!(rational.into_terms().is_empty());
    assert!(symbolic.into_terms().is_empty());
}

#[test]
fn fibration_bucket_namespace_is_part_of_complete_key_equality() {
    let left_symbol = Symbol::parse("x", "fibration_bucket_left").unwrap();
    let right_symbol = Symbol::parse("x", "fibration_bucket_right").unwrap();
    let left_ctx = PolyCtx::from_indeterminates([left_symbol.to_atom()]).unwrap();
    let right_ctx = PolyCtx::from_indeterminates([right_symbol.to_atom()]).unwrap();
    assert_eq!(left_ctx.vars(), right_ctx.vars());
    assert!(!left_ctx.is_compatible_with(&right_ctx));

    let left_key = vec![Word::from(vec![Rat::from_poly(
        Poly::generator(left_ctx.clone(), 0).unwrap(),
    )])];
    let right_key = vec![Word::from(vec![Rat::from_poly(
        Poly::generator(right_ctx.clone(), 0).unwrap(),
    )])];
    assert_eq!(left_key[0].to_string(), right_key[0].to_string());
    assert_ne!(left_key, right_key);

    let mut accumulator = FibBasisAcc::default();
    accumulator
        .add_with_digest(left_key.clone(), Rat::one(left_ctx), 0)
        .unwrap();
    accumulator
        .add_with_digest(right_key.clone(), Rat::one(right_ctx), 0)
        .unwrap();
    let terms = accumulator.into_terms();
    assert_eq!(terms.len(), 2);
    assert_eq!(terms[0].0, left_key);
    assert_eq!(terms[1].0, right_key);
}

#[test]
fn fibration_presentation_collisions_have_input_independent_structural_order() {
    let left_symbol = Symbol::parse("x", "fibration_order_left").unwrap();
    let right_symbol = Symbol::parse("x", "fibration_order_right").unwrap();
    let ctx =
        PolyCtx::from_indeterminates([left_symbol.to_atom(), right_symbol.to_atom()]).unwrap();
    let left_key = vec![Word::new(vec![Rat::from_poly(
        Poly::generator(ctx.clone(), 0).unwrap(),
    )])];
    let right_key = vec![Word::new(vec![Rat::from_poly(
        Poly::generator(ctx.clone(), 1).unwrap(),
    )])];
    assert_eq!(left_key[0].content_key(), right_key[0].content_key());
    assert_ne!(left_key, right_key);

    let build = |reverse: bool| {
        let mut accumulator = FibBasisAcc::default();
        let entries = if reverse {
            [right_key.clone(), left_key.clone()]
        } else {
            [left_key.clone(), right_key.clone()]
        };
        for key in entries {
            accumulator
                .add_with_digest(key, Rat::one(ctx.clone()), 0)
                .unwrap();
        }
        accumulator.into_terms()
    };
    assert_eq!(build(false), build(true));
}

#[test]
fn integer_letter_detection_uses_exact_symbolica_constants() {
    let (ctx, _) = setup();
    assert_eq!(
        integer_letter(&Rat::from_int(ctx.clone(), 7), "test").unwrap(),
        7
    );
    assert_eq!(
        integer_letter(&Rat::from_int(ctx.clone(), -2), "test").unwrap(),
        -2
    );
    assert!(integer_letter(&Rat::parse(ctx.clone(), "3/2").unwrap(), "test").is_err());
    assert!(integer_letter(&Rat::parse(ctx, "x").unwrap(), "test").is_err());
}
