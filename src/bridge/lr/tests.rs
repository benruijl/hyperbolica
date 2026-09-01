use serde_json::{Value, json};

use super::evaluate_supported;

#[test]
fn json_lr_request_accepts_explicit_euler_flag() {
    let request = json!({
        "op": "find_lr_orders",
        "xvars": ["x"],
        "groups": [["x"]],
        "euler_filter": true,
    });
    let response = evaluate_supported(&request, "find_lr_orders").unwrap();
    assert_eq!(response["op"], "find_lr_orders");
    assert!(response.get("nolr").is_some());
}

#[test]
fn verify_order_reports_malformed_permutations_in_band() {
    let request = json!({
        "op": "find_lr_orders",
        "xvars": ["x", "y"],
        "groups": [["x+y"]],
        "verify_order": ["x", "x"],
    });
    let response = evaluate_supported(&request, "find_lr_orders").unwrap();
    assert_eq!(response["order_is_lr"], false);
    assert_eq!(response["verify_malformed"], true);
    assert_eq!(response["verify_blocking_step"], -1);
    assert_eq!(response["verify_blocking_degree"], 0);
    assert_eq!(response["verify_forbidden_dep"], false);
    assert_eq!(response["verify_blocking_letter"], "");
    assert_eq!(response["best_order"], json!([]));
    assert_eq!(response["score"], Value::Null);
    assert_eq!(response["nolr"], false);
}

#[test]
fn verify_order_keeps_carry_inert_and_emits_the_compatibility_envelope() {
    let request = json!({
        "op": "find_lr_orders",
        "xvars": ["x", "y"],
        "groups": [["x^2*y+x+1", "x", "y"]],
        "verify_order": ["x", "y"],
        "algebraic_letters": true,
        "carry_discharge": true,
        "emit_sings": true,
    });
    let response = evaluate_supported(&request, "find_lr_orders").unwrap();
    assert_eq!(response["order_is_lr"], false);
    assert_eq!(response["verify_malformed"], false);
    assert_eq!(response["verify_blocking_step"], 0);
    assert_eq!(response["verify_blocking_degree"], 2);
    assert_eq!(response["verify_forbidden_dep"], true);
    assert_eq!(response["verify_blocking_letter"], "x^2*y + x + 1");
    assert_eq!(response["strategy"], "LR_OptOrdered");
    assert_eq!(response["root_polys"], json!([]));
    assert_eq!(response["carried_sqrts"], 0);
    assert_eq!(response["kin_sqrts"], 0);
    assert_eq!(response["terminal_quads"], 0);
    assert_eq!(response["carried_polys"], json!([]));
    assert_eq!(response["sings"], json!([]));
    assert_eq!(response["sings_total"], 0);
}

#[test]
fn verify_order_uses_the_requested_algebraic_degree_cap() {
    let strict = json!({
        "op": "find_lr_orders",
        "xvars": ["x", "y"],
        "groups": [["1+x+x^2", "x", "y"]],
        "verify_order": ["x", "y"],
    });
    let strict_response = evaluate_supported(&strict, "find_lr_orders").unwrap();
    assert_eq!(strict_response["order_is_lr"], false);
    assert_eq!(strict_response["strategy"], "LR_NoOpt");

    let mut algebraic = strict;
    algebraic["algebraic_letters"] = Value::Bool(true);
    algebraic["carry_discharge"] = Value::Bool(true);
    let algebraic_response = evaluate_supported(&algebraic, "find_lr_orders").unwrap();
    assert_eq!(algebraic_response["order_is_lr"], true);
    assert_eq!(algebraic_response["strategy"], "LR_OptOrdered");
}

#[test]
fn factor_table_formats_and_lexically_orients_only_at_the_wire_boundary() {
    // Encounter order is deliberately opposite to lexical order. The
    // structural table uses ID order internally, while the JSON adapter
    // must preserve HyperFLINT's historical f/g orientation and sign.
    let request = json!({
        "op": "factor_table",
        "xvars": ["x"],
        "groups": [["x+1", "x"]],
        "order": ["x"],
    });
    let response = evaluate_supported(&request, "factor_table").unwrap();

    assert_eq!(response["polys"], json!(["x + 1", "x"]));
    assert_eq!(response["pairs"].as_array().unwrap().len(), 1);
    assert_eq!(response["pairs"][0]["f"], 1);
    assert_eq!(response["pairs"][0]["g"], 0);
    assert_eq!(response["pairs"][0]["c"], "-1");
    assert_eq!(response["pairs"][0]["factors"], json!([]));
    assert!(
        response["singletons"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["deg"].as_u64().is_some_and(|degree| degree <= 1))
            .all(|entry| entry.get("disc").is_none()),
        "upstream omits discriminants for constant and linear singletons"
    );
}

#[test]
fn singularity_polynomials_are_structural_until_json_serialization() {
    let request = json!({
        "op": "find_lr_orders",
        "xvars": ["x"],
        "coeff_vars": ["s"],
        "groups": [["x+s"]],
        "emit_sings": true,
    });
    let response = evaluate_supported(&request, "find_lr_orders").unwrap();

    assert_eq!(response["sings"], json!(["s"]));
    assert_eq!(response["sings_total"], 1);
}

#[test]
fn registered_singularity_uses_its_legacy_wire_spelling() {
    let request = json!({
        "op": "find_lr_orders",
        "xvars": ["x"],
        "coeff_vars": ["Wm_1"],
        "groups": [["x+Wm_1"]],
        "emit_sings": true,
    });
    let response = evaluate_supported(&request, "find_lr_orders").unwrap();

    assert_eq!(response["sings"], json!(["Wm_1"]));
    assert_eq!(response["sings_total"], 1);
}

#[test]
fn registered_blocker_stays_typed_until_legacy_wire_serialization() {
    let request = json!({
        "op": "find_lr_orders",
        "xvars": ["x"],
        "coeff_vars": ["mzv_2"],
        "groups": [["x^2+mzv_2"]],
        "verify_order": ["x"],
    });
    let response = evaluate_supported(&request, "find_lr_orders").unwrap();
    let blocker = response["verify_blocking_letter"].as_str().unwrap();

    assert!(blocker.contains("mzv_2"));
    assert!(blocker.contains("x^2"));
    assert!(!blocker.contains("hyperbolica::MZV"));
}

#[test]
fn registered_factor_table_atoms_drive_legacy_lexical_pair_orientation() {
    // Native `MZV` sorts before native `Wp`, while their historical wire
    // spellings have the opposite byte order (`Wp_1` < `mzv_2`). Pair
    // orientation must therefore use the already-exported wire strings.
    let request = json!({
        "op": "factor_table",
        "xvars": ["x"],
        "coeff_vars": ["mzv_2", "Wp_1"],
        "groups": [["x+mzv_2", "x+Wp_1"]],
        "order": ["x"],
    });
    let response = evaluate_supported(&request, "factor_table").unwrap();
    let polynomials = response["polys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(
        polynomials
            .iter()
            .all(|polynomial| !polynomial.contains("hyperbolica::"))
    );

    let mzv_id = polynomials
        .iter()
        .position(|polynomial| polynomial.contains("mzv_2") && polynomial.contains('x'))
        .unwrap();
    let wp_id = polynomials
        .iter()
        .position(|polynomial| polynomial.contains("Wp_1") && polynomial.contains('x'))
        .unwrap();
    assert!(polynomials[wp_id] < polynomials[mzv_id]);

    let pair = response["pairs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pair| {
            let f = pair["f"].as_u64().unwrap() as usize;
            let g = pair["g"].as_u64().unwrap() as usize;
            (f == mzv_id && g == wp_id) || (f == wp_id && g == mzv_id)
        })
        .unwrap();
    assert_eq!(pair["f"], wp_id);
    assert_eq!(pair["g"], mzv_id);
}
