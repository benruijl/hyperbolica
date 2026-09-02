use symbolica::prelude::Symbol;

use super::*;

#[test]
fn legacy_name_resolution_rejects_unknown_and_ambiguous_diagnostics() {
    let left = Symbol::parse("x", "wire_delta_left").unwrap();
    let right = Symbol::parse("x", "wire_delta_right").unwrap();
    let ambiguous = PolyCtx::from_indeterminates([left.to_atom(), right.to_atom()]).unwrap();
    assert_eq!(ambiguous.vars(), &["x", "x"]);
    assert!(matches!(
        unique_diagnostic_variable_index(&ambiguous, "x"),
        Err(Error::InvalidInput(message)) if message.contains("ambiguous")
    ));
    assert!(matches!(
        unique_diagnostic_variable_index(&ambiguous, "missing"),
        Err(Error::UnknownVariable(name)) if name == "missing"
    ));

    let unambiguous = PolyCtx::from_indeterminates([right.to_atom()]).unwrap();
    assert_eq!(
        unique_diagnostic_variable_index(&unambiguous, "x").unwrap(),
        0
    );
}

#[test]
fn regulator_serialization_restores_legacy_lexical_factor_order() {
    let ctx = PolyCtx::new(["x"]).unwrap();
    let two = Word::new(vec![Rat::from_int(ctx.clone(), 2)]);
    let ten = Word::new(vec![Rat::from_int(ctx.clone(), 10)]);
    let regulator = vec![RegTerm {
        coef: Rat::one(ctx),
        key: vec![two.clone(), ten.clone()],
    }];

    assert_eq!(
        regulator_value(&regulator),
        json!([{"coef": "1", "key": [["10"], ["2"]]}])
    );
    assert_eq!(regulator[0].key, vec![two, ten]);
}

#[test]
fn registered_and_plain_symbols_with_one_wire_name_are_ambiguous() {
    let registered = crate::symbols::mzv_atom(&[2]);
    // Build the deliberate collision spelling dynamically so the source
    // audit still rejects accidental ad-hoc special symbols.
    let collision_name = ["mzv", "2"].join("_");
    let plain = Symbol::parse(&collision_name, crate::symbols::SYMBOL_NAMESPACE)
        .unwrap()
        .to_atom();
    let ctx = PolyCtx::from_indeterminates([registered, plain]).unwrap();

    assert_eq!(wire_context_variables(&ctx), ["mzv_2", "mzv_2"]);
    assert!(matches!(
        unique_diagnostic_variable_index(&ctx, "mzv_2"),
        Err(Error::InvalidInput(message)) if message.contains("ambiguous")
    ));
}

#[test]
fn regulator_wire_order_uses_emitted_special_names() {
    let special = crate::symbols::algebraic_atoms(1).minus;
    let ordinary = Symbol::parse("Wm0", crate::symbols::SYMBOL_NAMESPACE)
        .unwrap()
        .to_atom();
    let ctx = PolyCtx::from_indeterminates([special, ordinary]).unwrap();
    let special_word = Word::new(vec![Rat::from_poly(
        Poly::generator(ctx.clone(), 0).unwrap(),
    )]);
    let ordinary_word = Word::new(vec![Rat::from_poly(
        Poly::generator(ctx.clone(), 1).unwrap(),
    )]);
    let regulator = vec![RegTerm {
        coef: Rat::one(ctx),
        key: vec![special_word, ordinary_word],
    }];

    assert_eq!(
        regulator_value(&regulator),
        json!([{"coef": "1", "key": [["Wm0"], ["Wm_1"]]}])
    );
}

#[test]
fn inferred_context_order_is_lexical_not_symbol_registration_order() {
    // Register `z` first so raw Symbol IDs order these names opposite to
    // the legacy autoscan contract.
    Symbol::parse("wire_scan_z", crate::symbols::SYMBOL_NAMESPACE).unwrap();
    Symbol::parse("wire_scan_a", crate::symbols::SYMBOL_NAMESPACE).unwrap();

    let request = json!({});
    let ctx = context_for(&request, &["wire_scan_z+wire_scan_a"]).unwrap();
    assert_eq!(wire_context_variables(&ctx), ["wire_scan_a", "wire_scan_z"]);
}

#[test]
fn flat_json_optional_booleans_retain_the_default_for_other_types() {
    assert!(!optional_bool(&json!({"flag": "true"}), "flag", false).unwrap());
    assert!(optional_bool(&json!({"flag": 0}), "flag", true).unwrap());
    assert!(optional_bool(&json!({}), "flag", true).unwrap());
    assert!(!optional_bool(&json!({"flag": false}), "flag", true).unwrap());
}

#[test]
fn generic_wire_values_use_flint_polynomial_term_order() {
    let ctx = PolyCtx::new(["x"]).unwrap();
    let value = Rat::parse(ctx.clone(), "-2/(x^2-1)").unwrap();

    assert_eq!(value.wire_value(), "-2/(x^2 - 1)");
    assert_eq!(
        result_response("rat_add", &ctx, value),
        json!({"op": "rat_add", "result": "-2/(x^2 - 1)", "vars": ["x"]})
    );
}
