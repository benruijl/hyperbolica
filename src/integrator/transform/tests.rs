use std::sync::Arc;

use super::limits::one_regulator;
use super::word::identity_transform;
use super::{
    RegTerm, collect_regulator, reglim_word, shuffle_symbolic, transform_shuffle, transform_word,
};
use crate::core::{PolyCtx, Rat};
use crate::symbols::Word;

fn context() -> Arc<PolyCtx> {
    PolyCtx::new(["x", "y"]).unwrap()
}

fn rat(ctx: &Arc<PolyCtx>, expression: &str) -> Rat {
    Rat::parse(ctx.clone(), expression).unwrap()
}

fn word(ctx: &Arc<PolyCtx>, expressions: &[&str]) -> Word {
    Word::from(
        expressions
            .iter()
            .map(|expression| rat(ctx, expression))
            .collect::<Vec<_>>(),
    )
}

#[test]
fn canonical_keys_drop_identities_sort_and_collect() {
    let ctx = context();
    let first = word(&ctx, &["2"]);
    let second = word(&ctx, &["1"]);
    let regulator = vec![
        RegTerm {
            coef: rat(&ctx, "2"),
            key: vec![first.clone(), Word::default(), second.clone()],
        },
        RegTerm {
            coef: rat(&ctx, "-2"),
            key: vec![second, first],
        },
    ];
    assert!(collect_regulator(&regulator).unwrap().is_empty());
}

#[test]
fn symbolic_shuffle_multiplies_coefficients_and_keys() {
    let ctx = context();
    let left = vec![RegTerm {
        coef: rat(&ctx, "x"),
        key: vec![word(&ctx, &["2"])],
    }];
    let right = vec![RegTerm {
        coef: rat(&ctx, "3"),
        key: vec![word(&ctx, &["1"])],
    }];
    let output = shuffle_symbolic(&left, &right).unwrap();
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].coef, rat(&ctx, "3*x"));
    assert_eq!(output[0].key, vec![word(&ctx, &["1"]), word(&ctx, &["2"])]);
}

#[test]
fn regularized_limit_handles_empty_constant_and_scaled_words() {
    let ctx = context();
    let empty = reglim_word(&ctx, &Word::default(), 0).unwrap();
    assert_eq!(empty, one_regulator(&ctx, Vec::new()));
    assert!(
        reglim_word(&ctx, &word(&ctx, &["0", "0"]), 0)
            .unwrap()
            .is_empty()
    );

    let constant = reglim_word(&ctx, &word(&ctx, &["2", "y"]), 0).unwrap();
    assert_eq!(constant, one_regulator(&ctx, vec![word(&ctx, &["2", "y"])]));

    let scaled = reglim_word(&ctx, &word(&ctx, &["x", "-x"]), 0).unwrap();
    assert_eq!(scaled, one_regulator(&ctx, vec![word(&ctx, &["1", "-1"])]));
}

#[test]
fn transform_word_has_identity_constant_and_failed_tail_cases() {
    let ctx = context();
    assert_eq!(
        transform_word(&ctx, &Word::default(), 0).unwrap(),
        identity_transform(&ctx)
    );

    let constant = transform_word(&ctx, &word(&ctx, &["2"]), 0).unwrap();
    assert_eq!(constant.len(), 1);
    assert!(constant[0].shuffle.terms[0].word.is_empty());
    assert_eq!(
        constant[0].regulator,
        one_regulator(&ctx, vec![word(&ctx, &["2"])])
    );

    let failure = transform_word(&ctx, &word(&ctx, &["x", "0"]), 0).unwrap_err();
    assert!(failure.to_string().contains("$Failed"));
}

#[test]
fn transform_variable_word_extracts_linear_poles() {
    let ctx = context();
    let transformed = transform_word(&ctx, &word(&ctx, &["-x"]), 0).unwrap();
    assert_eq!(transformed.len(), 1);
    assert!(transformed[0].regulator[0].key.is_empty());
    assert_eq!(transformed[0].shuffle.terms.len(), 1);
    assert_eq!(transformed[0].shuffle.terms[0].coef, rat(&ctx, "-1"));
    assert_eq!(transformed[0].shuffle.terms[0].word, word(&ctx, &["0"]));
}

#[test]
fn transform_shuffle_collates_repeated_log_powers() {
    let ctx = context();
    let words = vec![word(&ctx, &["2"]), word(&ctx, &["2", "2"])];
    let transformed = transform_shuffle(&ctx, &words, 0).unwrap();
    assert_eq!(transformed.len(), 1);
    assert!(transformed[0].shuffle.terms[0].word.is_empty());
    assert_eq!(transformed[0].regulator.len(), 1);
    assert_eq!(
        transformed[0].regulator[0].coef.as_rat().unwrap(),
        rat(&ctx, "3")
    );
    assert_eq!(
        transformed[0].regulator[0].key,
        vec![word(&ctx, &["2", "2", "2"])]
    );
}
