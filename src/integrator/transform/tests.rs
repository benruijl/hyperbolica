use std::sync::Arc;

use symbolica::prelude::Symbol;

use super::collection::collect_regulator_with_digest;
use super::limits::{one_regulator, word_depends_on_variable};
use super::shuffle::group_log_powers_with_digest;
use super::word::{TransformCache, collect_result_rows_with_forced_collision, identity_transform};
use super::{
    RegTerm, RegTermSym, canonicalize_regkey, canonicalize_regulator, collect_regulator,
    regkey_content_key, reglim_word, regulator_sym_content_key, shuffle_symbolic,
    transform_shuffle, transform_word,
};
use crate::core::{Poly, PolyCtx, Rat, SymCoef};
use crate::error::Error;
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

fn namespaced_context(namespace: &'static str) -> Arc<PolyCtx> {
    let symbol = Symbol::parse("x", namespace).unwrap();
    PolyCtx::from_indeterminates([symbol.to_atom()]).unwrap()
}

fn generator(ctx: &Arc<PolyCtx>) -> Rat {
    Rat::from_poly(Poly::generator(ctx.clone(), 0).unwrap())
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
fn word_dependency_predicate_stays_on_native_symbolica_rationals() {
    let ctx = context();
    let independent = rat(&ctx, "(y+1)/(y-1)");
    let dependent = rat(&ctx, "(x+y)/(1+x*y)");

    assert!(!word_depends_on_variable(&Word::from(vec![independent.clone()]), 0).unwrap());
    assert!(
        word_depends_on_variable(&Word::from(vec![independent.clone(), dependent.clone()]), 0)
            .unwrap()
    );
    assert!(!independent.compatibility_views_initialized());
    assert!(!dependent.compatibility_views_initialized());
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

#[test]
fn forced_regkey_bucket_collision_only_combines_complete_keys() {
    let ctx = context();
    let first_key = vec![word(&ctx, &["1"])];
    let second_key = vec![word(&ctx, &["2"])];
    let regulator = vec![
        RegTerm {
            coef: rat(&ctx, "2"),
            key: first_key.clone(),
        },
        RegTerm {
            coef: rat(&ctx, "3"),
            key: second_key.clone(),
        },
        RegTerm {
            coef: rat(&ctx, "5"),
            key: first_key.clone(),
        },
    ];

    let output = collect_regulator_with_digest(&regulator, |_| 0).unwrap();
    assert_eq!(output.len(), 2);
    assert_eq!(output[0].key, first_key);
    assert_eq!(output[0].coef, rat(&ctx, "7"));
    assert_eq!(output[1].key, second_key);
    assert_eq!(output[1].coef, rat(&ctx, "3"));
}

#[test]
fn forced_log_letter_collision_keeps_distinct_powers_separate() {
    let ctx = context();
    let words = vec![
        word(&ctx, &["1"]),
        word(&ctx, &["2"]),
        word(&ctx, &["1", "1"]),
    ];

    let grouping = group_log_powers_with_digest(&ctx, &words, |_| 0).unwrap();
    assert!(grouping.repeated);
    assert!(grouping.combined.is_empty());
    assert_eq!(grouping.groups.len(), 2);
    assert_eq!(grouping.groups[0], (rat(&ctx, "1"), 3));
    assert_eq!(grouping.groups[1], (rat(&ctx, "2"), 1));
    assert_eq!(grouping.combinatorial_factor, rat(&ctx, "1/2"));
}

#[test]
fn transform_cache_resolves_forced_collision_by_word_and_namespace() {
    let left_ctx = namespaced_context("transform_cache_left");
    let right_ctx = namespaced_context("transform_cache_right");
    let left_word = Word::from(vec![generator(&left_ctx)]);
    let right_word = Word::from(vec![generator(&right_ctx)]);
    assert_eq!(left_word.to_string(), right_word.to_string());
    assert_ne!(left_word, right_word);

    let left_value = identity_transform(&left_ctx);
    let right_value = identity_transform(&right_ctx);
    let mut cache = TransformCache::default();
    cache.insert_in_bucket(&left_word, 0, left_value.clone(), 0);
    cache.insert_in_bucket(&right_word, 0, right_value.clone(), 0);

    assert_eq!(cache.get_in_bucket(&left_word, 0, 0), Some(&left_value));
    assert_eq!(cache.get_in_bucket(&right_word, 0, 0), Some(&right_value));
}

#[test]
fn transform_result_rows_resolve_regulator_and_word_collisions_structurally() {
    let ctx = context();
    let first_regulator = one_regulator(&ctx, Vec::new());
    let second_regulator = one_regulator(&ctx, vec![word(&ctx, &["3"])]);
    let first_word = word(&ctx, &["1"]);
    let second_word = word(&ctx, &["2"]);
    let entries = vec![
        (first_regulator.clone(), first_word.clone(), rat(&ctx, "2")),
        (second_regulator.clone(), first_word.clone(), rat(&ctx, "3")),
        (first_regulator.clone(), second_word.clone(), rat(&ctx, "5")),
        (first_regulator.clone(), first_word.clone(), rat(&ctx, "7")),
    ];

    let output = collect_result_rows_with_forced_collision(&entries).unwrap();
    assert_eq!(output.len(), 2);
    assert_eq!(output[0].regulator, first_regulator);
    assert_eq!(output[0].shuffle.terms.len(), 2);
    assert_eq!(output[0].shuffle.terms[0].word, first_word);
    assert_eq!(output[0].shuffle.terms[0].coef, rat(&ctx, "9"));
    assert_eq!(output[0].shuffle.terms[1].word, second_word);
    assert_eq!(output[0].shuffle.terms[1].coef, rat(&ctx, "5"));
    assert_eq!(output[1].regulator, second_regulator);
    assert_eq!(output[1].shuffle.terms[0].coef, rat(&ctx, "3"));
}

#[test]
fn collection_rejects_equal_spelling_from_distinct_symbol_namespaces() {
    let left_ctx = namespaced_context("transform_context_left");
    let right_ctx = namespaced_context("transform_context_right");
    assert_eq!(left_ctx.vars(), right_ctx.vars());
    assert!(!left_ctx.is_compatible_with(&right_ctx));

    let regulator = vec![
        RegTerm {
            coef: Rat::one(left_ctx.clone()),
            key: vec![Word::from(vec![generator(&left_ctx)])],
        },
        RegTerm {
            coef: Rat::one(right_ctx.clone()),
            key: vec![Word::from(vec![generator(&right_ctx)])],
        },
    ];
    assert!(matches!(
        collect_regulator(&regulator),
        Err(Error::ContextMismatch)
    ));
}

#[test]
fn collection_accepts_same_symbol_across_context_constructors() {
    let symbol = Symbol::parse("x", "transform_same_symbol").unwrap();
    let symbol_ctx = PolyCtx::from_symbols([symbol]).unwrap();
    let atom_ctx = PolyCtx::from_indeterminates([symbol.to_atom()]).unwrap();
    assert_ne!(symbol_ctx.vars(), atom_ctx.vars());
    assert!(symbol_ctx.is_compatible_with(&atom_ctx));

    let shared_key = vec![Word::from(vec![generator(&symbol_ctx)])];
    let equivalent_key = vec![Word::from(vec![generator(&atom_ctx)])];
    let output = collect_regulator(&vec![
        RegTerm {
            coef: Rat::one(symbol_ctx.clone()),
            key: shared_key.clone(),
        },
        RegTerm {
            coef: Rat::one(atom_ctx),
            key: equivalent_key,
        },
    ])
    .unwrap();
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].key, shared_key);
    assert_eq!(output[0].coef, Rat::from_int(symbol_ctx, 2));
}

#[test]
fn canonical_regulator_order_is_input_order_independent() {
    let ctx = context();
    let mut regulator = vec![
        RegTerm {
            coef: rat(&ctx, "2"),
            key: vec![word(&ctx, &["2"])],
        },
        RegTerm {
            coef: rat(&ctx, "3"),
            key: vec![word(&ctx, &["1"])],
        },
    ];
    let canonical = canonicalize_regulator(&regulator).unwrap();
    regulator.reverse();
    assert_eq!(canonicalize_regulator(&regulator).unwrap(), canonical);
}

#[test]
fn equal_presentation_keys_use_structural_ties_for_commutative_canonicalization() {
    let left_symbol = Symbol::parse("x", "regkey_order_left").unwrap();
    let right_symbol = Symbol::parse("x", "regkey_order_right").unwrap();
    let ctx =
        PolyCtx::from_indeterminates([left_symbol.to_atom(), right_symbol.to_atom()]).unwrap();
    assert_eq!(ctx.vars(), &["x", "x"]);
    let left = Word::new(vec![Rat::from_poly(
        Poly::generator(ctx.clone(), 0).unwrap(),
    )]);
    let right = Word::new(vec![Rat::from_poly(
        Poly::generator(ctx.clone(), 1).unwrap(),
    )]);
    assert_eq!(left.content_key(), right.content_key());
    assert_ne!(left, right);

    let forward = canonicalize_regkey(&vec![left.clone(), right.clone()]);
    let reverse = canonicalize_regkey(&vec![right.clone(), left.clone()]);
    assert_eq!(forward, reverse);

    let collected = collect_regulator(&vec![
        RegTerm {
            coef: Rat::from_int(ctx.clone(), 2),
            key: vec![left.clone(), right.clone()],
        },
        RegTerm {
            coef: Rat::from_int(ctx.clone(), 3),
            key: vec![right.clone(), left.clone()],
        },
    ])
    .unwrap();
    assert_eq!(collected.len(), 1);
    assert_eq!(collected[0].coef, Rat::from_int(ctx.clone(), 5));

    let mut presentation_collision = vec![
        RegTerm {
            coef: Rat::one(ctx.clone()),
            key: vec![left],
        },
        RegTerm {
            coef: Rat::one(ctx),
            key: vec![right],
        },
    ];
    let canonical = canonicalize_regulator(&presentation_collision).unwrap();
    presentation_collision.reverse();
    assert_eq!(
        canonicalize_regulator(&presentation_collision).unwrap(),
        canonical
    );
}

#[test]
fn mixed_compatible_context_spellings_have_identical_content_and_canonical_keys() {
    let symbol = Symbol::parse("x", "regkey_mixed_shared").unwrap();
    let qualified_ctx = PolyCtx::from_symbols([symbol]).unwrap();
    let stripped_ctx = PolyCtx::from_indeterminates([symbol.to_atom()]).unwrap();
    assert_ne!(qualified_ctx.vars(), stripped_ctx.vars());
    assert!(qualified_ctx.is_compatible_with(&stripped_ctx));

    let x_qualified = Word::new(vec![Rat::from_poly(
        Poly::generator(qualified_ctx.clone(), 0).unwrap(),
    )]);
    let x_stripped = Word::new(vec![Rat::from_poly(
        Poly::generator(stripped_ctx.clone(), 0).unwrap(),
    )]);
    let x_plus_one = symbol.to_atom() + 1;
    let plus_qualified = Word::new(vec![
        Rat::from_atom(qualified_ctx, x_plus_one.as_view()).unwrap(),
    ]);
    let plus_stripped = Word::new(vec![
        Rat::from_atom(stripped_ctx, x_plus_one.as_view()).unwrap(),
    ]);

    assert_eq!(x_qualified, x_stripped);
    assert_eq!(plus_qualified, plus_stripped);
    assert_eq!(x_qualified.content_key(), x_stripped.content_key());
    assert_eq!(plus_qualified.content_key(), plus_stripped.content_key());

    let forward_input = vec![x_qualified, plus_stripped];
    let reversed_input = vec![plus_qualified, x_stripped];
    assert_eq!(
        regkey_content_key(&forward_input),
        regkey_content_key(&reversed_input)
    );
    let forward = canonicalize_regkey(&forward_input);
    let reversed = canonicalize_regkey(&reversed_input);
    assert_eq!(forward, reversed);
}

#[test]
fn symbolic_regulator_content_uses_native_prefactor_and_delta_spellings() {
    let symbol = Symbol::parse("x", "regulator_content_shared").unwrap();
    let qualified_ctx = PolyCtx::from_symbols([symbol]).unwrap();
    let stripped_ctx = PolyCtx::from_indeterminates([symbol.to_atom()]).unwrap();
    let qualified_prefactor = generator(&qualified_ctx);
    let stripped_prefactor = generator(&stripped_ctx);
    let mut qualified = SymCoef::from_rat(&qualified_prefactor);
    let mut stripped = SymCoef::from_rat(&stripped_prefactor);
    qualified = qualified
        .try_mul(&SymCoef::delta_factor(qualified_ctx, 0).unwrap())
        .unwrap();
    stripped = stripped
        .try_mul(&SymCoef::delta_factor(stripped_ctx, 0).unwrap())
        .unwrap();

    let qualified_regulator = vec![RegTermSym {
        coef: qualified,
        key: Vec::new(),
    }];
    let stripped_regulator = vec![RegTermSym {
        coef: stripped,
        key: Vec::new(),
    }];
    assert_eq!(
        regulator_sym_content_key(&qualified_regulator).unwrap(),
        regulator_sym_content_key(&stripped_regulator).unwrap()
    );
}
