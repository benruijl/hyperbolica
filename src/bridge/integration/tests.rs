use serde_json::json;
use symbolica::prelude::symbol;

use super::*;
use crate::reduce::{mzv_constant_atom, standard_mzv_reductions};
use crate::symbols::{algebraic_atoms, mzv_atom};

#[test]
fn rational_and_word_inputs_use_slim_contexts_without_explicit_constants() {
    let request = json!({"vars": ["x", "y"]});
    let (ctx, table) =
        integration_context(&request, &["x".into()], &["1/((1+x)*(1+x+y))"], false).unwrap();
    assert_eq!(ctx.len(), 2);
    assert!(crate::reduce::period_tuples_active(&ctx, &table));
}

#[test]
fn explicit_constants_custom_tables_and_algebraic_letters_keep_the_basis() {
    let table = standard_mzv_reductions();
    let variables = vec!["x".into()];
    for expression in [
        "mzv_2/(1+x)",
        "Log2+x",
        "1/(x-Wm_1)",
        "MZV[3]/(1+x)",
        "1/(x-Wm[1])",
    ] {
        assert!(!use_period_tuple_context(
            &table,
            &variables,
            &[expression],
            false
        ));
    }
    assert!(!use_period_tuple_context(
        &table,
        &variables,
        &["1/(1+x)"],
        true
    ));
    assert!(!use_period_tuple_context(
        &MzvReductionTable::default(),
        &variables,
        &["1/(1+x)"],
        false,
    ));
}

#[test]
fn slim_expression_preserves_the_complete_wide_regulator() {
    let mut request = json!({
        "op": "hyperflint", "vars": ["x"], "vars_int": ["x"],
        "expr": "Log[1+x]/(x*(1+x))", "parallel": false,
    });
    let slim = evaluate_supported(&request, "hyperflint").unwrap();
    assert!(slim.get("failed").is_none(), "{slim}");
    assert_eq!(slim["vars"], json!(["x"]));
    let coefficient =
        crate::reduce::mzv_expression_atom(slim["result"][0]["coef"].as_str().unwrap()).unwrap();
    assert_eq!(coefficient, symbolica::prelude::Atom::one());
    assert_eq!(slim["result"].as_array().unwrap().len(), 1);
    // Upstream retains G[0,-1] as a terminal period key here.
    assert_eq!(slim["result"][0]["key"], json!([["0", "-1"]]));

    request["vars"] = json!(["x", "mzv_2"]);
    let wide = evaluate_supported(&request, "hyperflint").unwrap();
    assert!(wide.get("failed").is_none(), "{wide}");
    let wide_coefficient =
        crate::reduce::mzv_expression_atom(wide["result"][0]["coef"].as_str().unwrap()).unwrap();
    assert_eq!(coefficient, wide_coefficient);
    assert_eq!(slim["result"][0]["key"], wide["result"][0]["key"]);
}

#[test]
fn indexed_input_constant_retains_the_complete_period_basis() {
    let request = json!({
        "op": "hyperflint", "vars": ["x"], "vars_int": ["x"],
        "expr": "MZV[3]*Log[1+x]/(x*(1+x))", "parallel": false,
    });
    let response = evaluate_supported(&request, "hyperflint").unwrap();
    assert!(response.get("failed").is_none(), "{response}");
    let coefficient =
        crate::reduce::mzv_expression_atom(response["result"][0]["coef"].as_str().unwrap())
            .unwrap();
    assert_eq!(coefficient, mzv_atom(&[3]));
    assert_eq!(response["result"].as_array().unwrap().len(), 1);
    assert_eq!(response["result"][0]["key"], json!([["0", "-1"]]));
    assert!(
        response["vars"]
            .as_array()
            .unwrap()
            .contains(&json!("mzv_2"))
    );
}

#[test]
fn standard_rule_lhs_is_reserved_without_widening_the_context() {
    let x = symbol!("bridge_spectator_x").to_atom();
    let y = symbol!("bridge_spectator_y").to_atom();
    let lhs = mzv_constant_atom("mzv_4").unwrap();
    let ctx = PolyCtx::from_indeterminates([x, y, lhs]).unwrap();
    assert_eq!(ctx.len(), 3);

    let spectators =
        hyperflint_spectator_indices(&ctx, &[0], &standard_mzv_reductions(), false).unwrap();
    assert_eq!(spectators, [1]);
    assert_eq!(ctx.len(), 3);
}

#[test]
fn interval_bounds_participate_in_standard_mzv_narrowing() {
    let request = json!({
        "vars": ["x"],
        "vars_int_from": ["mzv_4"],
        "vars_int_to": ["1"],
    });
    let expressions = payload_strings(&request, &["vars_int_from", "vars_int_to"]);
    let (ctx, _) = integration_context(&request, &["x".into()], &expressions, false).unwrap();
    assert!(
        ctx.index_of_indeterminate(mzv_constant_atom("mzv_4").unwrap().as_view())
            .is_some()
    );
}

#[test]
fn every_registered_mzv_and_algebraic_atom_is_reserved_structurally() {
    let x = symbol!("bridge_structural_spectator_x").to_atom();
    let mzv = mzv_atom(&[999]);
    let algebraic = algebraic_atoms(71).minus;
    let ctx = PolyCtx::from_indeterminates([x, mzv, algebraic]).unwrap();
    let spectators =
        hyperflint_spectator_indices(&ctx, &[0], &standard_mzv_reductions(), false).unwrap();
    assert!(spectators.is_empty());
}
#[test]
fn bare_rational_input_retains_powered_denominator_blocks() {
    let ctx = PolyCtx::new(["x", "y", "z"]).unwrap();
    let entry = parse_factored_rational_input(&ctx, "(1+x+y)^3/(1+x+y+z)^5").unwrap();
    assert!(entry.coef.is_one());
    let factored = entry.factored_coefficient().unwrap();
    assert_eq!(factored.den_factors().len(), 1);
    assert_eq!(factored.den_factors()[0].exp, 5);
    assert_eq!(factored.den_factors()[0].base.n_terms(), 4);
    assert_eq!(factored.numerator().n_terms(), 10);
}

#[test]
fn deferred_rational_bridge_matches_eager_wordlist_inputs() {
    for (expression, variables, checked) in [
        ("1/((1+x)^2*(1+y)^2)", vec!["x", "y"], true),
        ("(1+2*x)/(1+x)^3", vec!["x"], false),
        ("1/(1+x)^2", vec!["x"], true),
    ] {
        let mut request = json!({
            "op":"hyperflint", "vars":variables, "vars_int":variables,
            "f":expression, "parallel":false, "check_divergences":checked,
        });
        let deferred = evaluate_supported(&request, "hyperflint").unwrap();
        assert!(deferred.get("failed").is_none(), "{deferred}");
        assert!(deferred.get("divergent").is_none(), "{deferred}");
        request.as_object_mut().unwrap().remove("f");
        request["wordlist"] = json!([{"coef":expression,"shuffle":[]}]);
        let eager = evaluate_supported(&request, "hyperflint").unwrap();
        assert_eq!(deferred["result"], eager["result"], "{expression}");
        assert_eq!(deferred["vars"], eager["vars"]);
    }
}
