//! End-to-end checks for the public JSON transport.
//!
//! This is deliberately one test: restricted Symbolica builds bind execution
//! to one instance, so representative requests are run serially.

use std::collections::BTreeSet;
use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn eval(request: &Value) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hyperflint"))
        .arg("eval-json")
        .env("SYMBOLICA_HIDE_BANNER", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hyperflint eval-json");

    serde_json::to_writer(
        child.stdin.as_mut().expect("piped stdin is available"),
        request,
    )
    .expect("serialize JSON request");
    child
        .stdin
        .as_mut()
        .expect("piped stdin is available")
        .write_all(b"\n")
        .expect("terminate JSON request");
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("wait for hyperflint");
    assert!(
        output.status.success(),
        "request failed\nrequest: {request}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let response: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "response is not JSON: {error}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        )
    });
    assert_eq!(response["op"], request["op"]);
    assert!(response.get("error").is_none(), "{response}");
    response
}

fn assert_rat_equivalent(actual: &str, expected: &str, variables: &[&str]) {
    let response = eval(&json!({
        "op": "rat_sub",
        "a": actual,
        "b": expected,
        "vars": variables,
    }));
    assert_eq!(response["result"], "0", "{actual} != {expected}");
}

fn string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("expected JSON array")
        .iter()
        .map(|entry| entry.as_str().expect("expected JSON string").to_owned())
        .collect()
}

fn minimal_dispatch_requests() -> Vec<Value> {
    vec![
        json!({"op": "add", "a": "x", "b": "1", "vars": ["x"]}),
        json!({
            "op": "algebraic_letters_allocate",
            "polynomial": "x^2+1",
            "var": "x",
            "vars": ["x"],
        }),
        json!({"op": "algebraic_letters_clear"}),
        json!({"op": "algebraic_letters_show"}),
        json!({"op": "back_substitute", "expr": "x", "vars": ["x"]}),
        json!({
            "op": "break_up_contour_sym",
            "wl": [],
            "on_axis": [],
            "vars": ["x"],
        }),
        json!({"op": "collect_words", "wl": [], "vars": ["x"]}),
        json!({
            "op": "combine_wm_wp_ratios",
            "expr": "x",
            "vars": ["x"],
        }),
        json!({"op": "concat_mul", "a": [], "b": [], "vars": ["x"]}),
        json!({"op": "convert_1inf_to_01", "wl": [], "vars": ["x"]}),
        json!({
            "op": "convert_ab_to_zero_inf",
            "A": "0",
            "B": "1",
            "wl": [],
            "vars": ["x"],
        }),
        json!({
            "op": "derivative",
            "a": "x^2",
            "var": "x",
            "vars": ["x"],
        }),
        json!({
            "op": "differentiate_wordlist",
            "wl": [],
            "var": "x",
            "vars": ["x"],
        }),
        json!({
            "op": "diff_hlog",
            "z": "x",
            "word": [],
            "var": "x",
            "vars": ["x"],
        }),
        json!({
            "op": "diff_mpl",
            "ns": [1],
            "zs": ["x"],
            "var": "x",
            "vars": ["x"],
        }),
        json!({"op": "divexact", "a": "x^2", "b": "x", "vars": ["x"]}),
        json!({"op": "eval", "a": "x+1", "values": ["2"], "vars": ["x"]}),
        json!({"op": "expand_inf_word", "word": [], "min_order": 0}),
        json!({"op": "expand_zero_word", "word": [], "min_order": 0}),
        json!({
            "op": "find_lr_orders_scan",
            "xvars": ["x"],
            "groups": [["x"]],
            "exps": [[[-1, 0]]],
        }),
        json!({
            "op": "hlog_series",
            "arg": "1",
            "word": [],
            "var": "x",
            "order": 0,
            "vars": ["x"],
        }),
        json!({
            "op": "hlog_zero_expand",
            "arg": "x",
            "word": [],
            "order": 0,
            "vars": ["x"],
        }),
        json!({
            "op": "mpl_series",
            "ns": [],
            "zs": [],
            "var": "x",
            "order": 0,
            "vars": ["x"],
        }),
        json!({"op": "mpl_sum", "ns": [1], "zs": ["x"], "max_n": 1}),
        json!({"op": "neg", "a": "x", "vars": ["x"]}),
        json!({"op": "pole_degree", "f": "1/x^2", "var": "x", "vars": ["x"]}),
        json!({"op": "rat_div", "a": "x", "b": "x+1", "vars": ["x"]}),
        json!({"op": "rat_residue", "f": "1/x", "var": "x", "vars": ["x"]}),
        json!({"op": "reg0", "wl": [], "vars": ["x"]}),
        json!({"op": "reg_head", "wl": [], "vars": ["x"]}),
        json!({"op": "reg_tail", "wl": [], "vars": ["x"]}),
        json!({"op": "regzero_word", "word": [], "vars": ["x"]}),
        json!({"op": "shuffle_product", "a": [], "b": [], "vars": ["x"]}),
        json!({"op": "shuffle_symbolic", "a": [], "b": [], "vars": ["x"]}),
        json!({
            "op": "simplify_with_vieta",
            "expr": "x",
            "vars": ["x"],
        }),
        json!({"op": "sub", "a": "x", "b": "1", "vars": ["x"]}),
        json!({
            "op": "sym_arith",
            "mode": "add",
            "a": [{"prefactor": "1"}],
            "b": [{"prefactor": "2"}],
            "vars": ["x"],
        }),
        json!({"op": "zero_inf_period", "word": [], "vars": ["x"]}),
        json!({"op": "zero_one_period", "word": [], "vars": ["x"]}),
    ]
}

#[test]
fn representative_json_cli_schemas_are_stable() {
    // Keep a minimal valid request for every dispatch arm that is not already
    // exercised by a more specific JSON assertion below. This table is about
    // transport parsing and routing; kernel behavior has focused unit tests.
    let minimal_requests = minimal_dispatch_requests();
    assert_eq!(minimal_requests.len(), 39);
    let minimal_operations = minimal_requests
        .iter()
        .map(|request| request["op"].as_str().expect("operation string"))
        .collect::<BTreeSet<_>>();
    assert_eq!(minimal_operations.len(), minimal_requests.len());
    for request in minimal_requests {
        eval(&request);
    }

    // Polynomial/rational algebra: compare values algebraically so harmless
    // printer changes do not turn the integration test into a snapshot test.
    let product = eval(&json!({
        "op": "mul",
        "a": "x+y",
        "b": "x-y",
        "vars": ["x", "y"],
        "schema_version_min": 2,
    }));
    assert_eq!(product["vars"], json!(["x", "y"]));
    assert_rat_equivalent(
        product["result"].as_str().expect("polynomial result"),
        "x^2-y^2",
        &["x", "y"],
    );

    let fractions = eval(&json!({
        "op": "partial_fractions",
        "f": "2*x/(x^2-1)",
        "var": "x",
        "vars": ["x"],
    }));
    assert_eq!(fractions["var"], "x");
    assert_eq!(fractions["polynomial_part"], "0");
    let poles = fractions["poles"]
        .as_array()
        .expect("partial-fraction pole array");
    assert_eq!(poles.len(), 2);
    let pole_names = poles
        .iter()
        .map(|pole| pole["pole"].as_str().expect("pole string"))
        .collect::<BTreeSet<_>>();
    assert_eq!(pole_names, BTreeSet::from(["-1", "1"]));
    assert!(poles.iter().all(|pole| pole["multiplicity"] == 1));
    assert!(fractions.get("algebraic_letters").is_none());

    // The opt-in schema adds an allocation table and exposes only the
    // legacy wire spellings of the structural Wm/Wp function atoms.
    let algebraic_fractions = eval(&json!({
        "op": "partial_fractions",
        "f": "1/(x^2+1)",
        "var": "x",
        "vars": ["x"],
        "introduce_algebraic_letters": true,
    }));
    let algebraic_poles = algebraic_fractions["poles"]
        .as_array()
        .expect("algebraic partial-fraction pole array");
    assert_eq!(algebraic_poles.len(), 2);
    assert_eq!(
        algebraic_poles
            .iter()
            .map(|pole| pole["pole"].as_str().expect("algebraic pole"))
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["Wm_1", "Wp_1"]),
    );
    let allocations = algebraic_fractions["algebraic_letters"]
        .as_array()
        .expect("algebraic-letter allocation table");
    assert_eq!(allocations.len(), 1);
    assert_eq!(allocations[0]["idx"], 1);
    assert_eq!(allocations[0]["wm"], "Wm_1");
    assert_eq!(allocations[0]["wp"], "Wp_1");

    // Word algebra and expression conversion.
    let shuffled = eval(&json!({
        "op": "shuffle_words",
        "v": ["0"],
        "w": ["1"],
        "vars": ["x"],
    }));
    let terms = shuffled["result"].as_array().expect("word-list result");
    assert_eq!(terms.len(), 2);
    let words = terms
        .iter()
        .map(|term| string_array(&term["word"]))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        words,
        BTreeSet::from([vec!["0".into(), "1".into()], vec!["1".into(), "0".into()]])
    );
    assert!(terms.iter().all(|term| term["coef"] == "1"));

    let converted = eval(&json!({
        "op": "convert_zero_one",
        "wl": [{"coef": "1", "word": ["0", "1"]}],
        "vars": ["x"],
    }));
    let converted_terms = converted["result"]
        .as_array()
        .expect("converted word list")
        .iter()
        .map(|term| {
            (
                term["coef"].as_str().expect("coefficient").to_owned(),
                string_array(&term["word"]),
            )
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        converted_terms,
        BTreeSet::from([
            ("1".into(), vec!["0".into(), "0".into()]),
            ("-1".into(), vec!["0".into(), "1".into()]),
            ("-1".into(), vec!["1/2".into(), "0".into()]),
            ("1".into(), vec!["1/2".into(), "1".into()]),
        ])
    );

    let parsed = eval(&json!({
        "op": "parse_expr",
        "expr": "Hlog[x,[0,1]]",
        "vars": ["x"],
    }));
    assert_eq!(parsed["canonical"], "Hlog[x,[0,1]]");

    let scaled_rational = eval(&json!({
        "op": "parse_expr",
        "expr": "(2*x+2)/(2*y+2)",
        "vars": ["x", "y"],
    }));
    let primitive_rational = eval(&json!({
        "op": "parse_expr",
        "expr": "(x+1)/(y+1)",
        "vars": ["x", "y"],
    }));
    assert_eq!(
        scaled_rational["canonical"],
        primitive_rational["canonical"]
    );

    let identity = eval(&json!({
        "op": "convert_to_hlog_reg_inf",
        "expr": "Hlog[x,[]]",
        "vars": ["x"],
    }));
    assert_ne!(identity["failed"], true);
    assert_eq!(identity["result"], json!([{"coef": "1", "key": []}]));

    // Linear reducibility and reduction. Dynamic timings are checked by type,
    // while a tied order is accepted as long as it is a full permutation.
    let lr = eval(&json!({
        "op": "find_lr_orders",
        "xvars": ["x", "y"],
        "groups": [["x+y", "1-x", "1-y"]],
    }));
    assert_eq!(lr["schema_version"], 2);
    assert_eq!(lr["nolr"], false);
    assert_eq!(lr["nXVars"], 2);
    assert_eq!(lr["nGroups"], 1);
    assert_eq!(lr["nPolys"], json!([3]));
    assert!(lr["timing_compute_s"].as_f64().is_some());
    let mut order = string_array(&lr["best_order"]);
    order.sort_unstable();
    assert_eq!(order, ["x", "y"]);

    // All six rows of the public strategy truth table must survive the JSON
    // boundary. The method hint matters only on the NOLR fallback path.
    for (request, expected) in [
        (
            json!({
                "op": "find_lr_orders",
                "xvars": ["x1", "x2"],
                "polys": ["x1", "x2", "x1+x2"],
            }),
            "LR_NoOpt",
        ),
        (
            json!({
                "op": "find_lr_orders",
                "xvars": ["x1", "x2"],
                "polys": ["x1", "x2", "x1+x2"],
                "method_lr_hint": "Espresso",
            }),
            "LR_NoOpt",
        ),
        (
            json!({
                "op": "find_lr_orders",
                "xvars": ["x1", "x2"],
                "polys": ["x1", "x2", "x1+x2"],
                "algebraic_letters": true,
            }),
            "LR_OptOrdered",
        ),
        (
            json!({
                "op": "find_lr_orders",
                "xvars": ["x1", "x2"],
                "polys": ["x1", "x2", "x1+x2"],
                "algebraic_letters": true,
                "method_lr_hint": "Espresso",
            }),
            "LR_OptOrdered",
        ),
        (
            json!({
                "op": "find_lr_orders",
                "xvars": ["x1", "x2"],
                "polys": ["1+x1^2+x2^2"],
            }),
            "Fubini_Lungo",
        ),
        (
            json!({
                "op": "find_lr_orders",
                "xvars": ["x1", "x2"],
                "polys": ["1+x1^2+x2^2"],
                "method_lr_hint": "Espresso",
            }),
            "Fubini_Espresso",
        ),
    ] {
        assert_eq!(eval(&request)["strategy"], expected, "request: {request}");
    }

    // Carry-discharge is default-off and flips this independently derived
    // two-variable Cheng--Wu face from NOLR to an exact one-root order.
    let carry_base = json!({
        "op": "find_lr_orders",
        "xvars": ["x1", "x2"],
        "coeff_vars": ["s"],
        "polys": ["x1+x2+1", "x1^2+x2^2+x1*x2+s"],
        "algebraic_letters": true,
    });
    let carry_default = eval(&carry_base);
    let mut carry_off_request = carry_base.clone();
    carry_off_request["carry_discharge"] = Value::Bool(false);
    let carry_off = eval(&carry_off_request);
    assert_eq!(carry_default["nolr"], true);
    assert_eq!(carry_off["nolr"], true);
    assert_eq!(carry_default["best_order"], carry_off["best_order"]);
    assert_eq!(carry_default["strategy"], carry_off["strategy"]);

    let mut carry_on_request = carry_base;
    carry_on_request["carry_discharge"] = Value::Bool(true);
    let carry_on = eval(&carry_on_request);
    assert_eq!(carry_on["nolr"], false);
    assert_eq!(carry_on["best_order"], json!(["x1", "x2"]));
    assert_eq!(carry_on["carried_sqrts"], 1);

    // Selection is lexicographic in carried-root count before score, and a
    // cubic in every possible first pivot must never be over-accepted.
    let uncarried_wins = eval(&json!({
        "op": "find_lr_orders",
        "xvars": ["x", "y"],
        "coeff_vars": ["s"],
        "polys": [
            "x+y+1",
            "x^2+x*y+y+1",
            "y+(s^4+s^3+s^2+s+1)"
        ],
        "algebraic_letters": true,
        "carry_discharge": true,
    }));
    assert_eq!(uncarried_wins["nolr"], false);
    assert_eq!(uncarried_wins["carried_sqrts"], 0);

    let cubic = eval(&json!({
        "op": "find_lr_orders",
        "xvars": ["x", "y"],
        "coeff_vars": ["s"],
        "polys": ["x^3*y^3+x^3+y^3+s"],
        "algebraic_letters": true,
        "carry_discharge": true,
    }));
    assert_eq!(cubic["nolr"], true);

    // Specific-order LR certification is search-free on the common linear
    // path and retains HyperFLINT's inert search envelope.
    let verified = eval(&json!({
        "op": "find_lr_orders",
        "xvars": ["x", "y"],
        "coeff_vars": [],
        "groups": [["x", "1+x", "y"], ["y", "1+y", "x"]],
        "verify_order": ["x", "y"],
    }));
    assert_eq!(verified["order_is_lr"], true);
    assert_eq!(verified["verify_malformed"], false);
    assert_eq!(verified["verify_blocking_step"], -1);
    assert_eq!(verified["verify_blocking_degree"], 0);
    assert_eq!(verified["verify_forbidden_dep"], false);
    assert_eq!(verified["verify_blocking_letter"], "");
    assert_eq!(verified["best_order"], json!([]));
    assert_eq!(verified["score"], Value::Null);
    assert_eq!(verified["nolr"], false);
    assert_eq!(verified["strategy"], "LR_NoOpt");

    // Verification deliberately does not execute carry-discharge: a
    // quadratic involving a later pivot remains a loud, diagnosed rejection.
    let forbidden = eval(&json!({
        "op": "find_lr_orders",
        "xvars": ["x", "y"],
        "coeff_vars": [],
        "groups": [["x^2*y+x+1", "x", "y"]],
        "verify_order": ["x", "y"],
        "algebraic_letters": true,
        "carry_discharge": true,
    }));
    assert_eq!(forbidden["order_is_lr"], false);
    assert_eq!(forbidden["verify_blocking_step"], 0);
    assert_eq!(forbidden["verify_blocking_degree"], 2);
    assert_eq!(forbidden["verify_forbidden_dep"], true);
    assert_eq!(forbidden["verify_blocking_letter"], "x^2*y + x + 1");
    assert_eq!(forbidden["strategy"], "LR_OptOrdered");
    assert_eq!(forbidden["carried_sqrts"], 0);
    assert_eq!(forbidden["carried_polys"], json!([]));

    let malformed = eval(&json!({
        "op": "find_lr_orders",
        "xvars": ["x", "y"],
        "groups": [["x+y"]],
        "verify_order": ["x", "x"],
    }));
    assert_eq!(malformed["order_is_lr"], false);
    assert_eq!(malformed["verify_malformed"], true);
    assert_eq!(malformed["verify_blocking_step"], -1);

    let reduced = eval(&json!({
        "op": "apply_mzv_reductions",
        "f": "mzv_m2",
        "vars": ["mzv_m2", "mzv_2"],
    }));
    assert!(
        string_array(&reduced["vars"])
            .iter()
            .any(|variable| variable == "mzv_2")
    );
    assert!(string_array(&reduced["vars"]).len() < 20);
    assert_rat_equivalent(
        reduced["result"].as_str().expect("reduced expression"),
        "-1/2*mzv_2",
        &["mzv_2"],
    );

    // Primitive construction, one integration step, and the multi-variable
    // driver cover the three public integration response shapes.
    let primitive = eval(&json!({
        "op": "integrate_ii",
        "var": "x",
        "vars": ["x"],
        "wl": [{"coef": "1/(1-x)", "word": []}],
    }));
    assert_ne!(primitive["failed"], true);
    let primitive_terms = primitive["result"].as_array().expect("primitive terms");
    assert_eq!(primitive_terms.len(), 1);
    assert_eq!(primitive_terms[0]["word"], json!(["1"]));
    assert_rat_equivalent(
        primitive_terms[0]["coef"]
            .as_str()
            .expect("primitive coefficient"),
        "-1",
        &["x"],
    );

    let step = eval(&json!({
        "op": "integration_step",
        "var": "x",
        "vars": ["x"],
        "parallel": false,
        "check_divergences": true,
        "wordlist": [{"coef": "1/(x+1)^2", "shuffle": []}],
    }));
    assert_ne!(step["failed"], true);
    assert_ne!(step["divergent"], true);
    assert_eq!(step["result"], json!([{"coef": "1", "key": []}]));

    let step_with_spectator = eval(&json!({
        "op": "integration_step",
        "var": "x",
        "remaining_vars": ["y"],
        "vars": ["x", "y"],
        "parallel": false,
        "check_divergences": true,
        "wordlist": [{"coef": "1/((x+1)*(x+y))", "shuffle": []}],
    }));
    assert_ne!(step_with_spectator["failed"], true);
    assert_ne!(step_with_spectator["divergent"], true);
    assert!(!step_with_spectator["result"].as_array().unwrap().is_empty());

    let duplicate_remaining = eval(&json!({
        "op": "integration_step",
        "var": "x",
        "remaining_vars": ["y", "y"],
        "vars": ["x", "y"],
        "wordlist": [],
    }));
    assert_eq!(duplicate_remaining["failed"], true);
    assert!(
        duplicate_remaining["reason"]
            .as_str()
            .unwrap()
            .contains("listed more than once")
    );

    let current_as_remaining = eval(&json!({
        "op": "integration_step",
        "var": "x",
        "remaining_vars": ["x"],
        "vars": ["x"],
        "wordlist": [],
    }));
    assert_eq!(current_as_remaining["failed"], true);
    assert!(
        current_as_remaining["reason"]
            .as_str()
            .unwrap()
            .contains("current integration variable")
    );

    let integrated = eval(&json!({
        "op": "hyperflint",
        "vars": ["x", "y"],
        "vars_int": ["x", "y"],
        "f": "1/((1+x)^2*(1+y)^2)",
        "parallel": false,
        "check_divergences": true,
    }));
    assert_ne!(integrated["failed"], true);
    assert_ne!(integrated["divergent"], true);
    assert_eq!(integrated["result"], json!([{"coef": "1", "key": []}]));
    assert!(integrated["timing_compute_s"].as_f64().is_some());

    let integrated_with_spectator = eval(&json!({
        "op": "hyperflint",
        "vars": ["x", "y"],
        "vars_int": ["x"],
        "f": "1/((x+1)*(x+y))",
        "parallel": false,
        "check_divergences": true,
    }));
    assert_ne!(integrated_with_spectator["failed"], true);
    assert_ne!(integrated_with_spectator["divergent"], true);
    assert!(
        !integrated_with_spectator["result"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let parametric_quadratic = eval(&json!({
        "op": "hyperflint",
        "vars": ["x", "y"],
        "vars_int": ["x"],
        "f": "1/(x^2+y)",
        "algebraic_letters": true,
        "parallel": false,
    }));
    assert_ne!(parametric_quadratic["failed"], true);
    let parametric_allocations = parametric_quadratic["algebraic_letters"]
        .as_array()
        .expect("parametric quadratic allocation table");
    assert_eq!(parametric_allocations.len(), 1);
    assert!(
        parametric_allocations[0]["polynomial"]
            .as_str()
            .unwrap()
            .contains('y')
    );

    let discovered_spectator = eval(&json!({
        "op": "hyperflint",
        "vars_int": ["x"],
        "f": "1/((x+1)*(x+y))",
        "parallel": false,
        "check_divergences": true,
    }));
    assert_ne!(discovered_spectator["failed"], true);
    assert_ne!(discovered_spectator["divergent"], true);
    assert!(
        !discovered_spectator["result"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // Standard-table narrowing must inspect expressions below the top-level
    // request. Each response retains the mentioned rule LHS, while avoiding a
    // 700-variable polynomial context.
    let nested_mzv_requests = [
        json!({
            "op": "evaluate_periods",
            "vars": ["x"],
            "regulator": [{"coef": "mzv_4", "key": []}],
        }),
        json!({
            "op": "test_zero_function",
            "vars": ["x"],
            "regulator": [{"coef": "mzv_4", "key": []}],
        }),
        json!({
            "op": "fibration_basis",
            "vars": ["x"],
            "vars_int": [],
            "wordlist": [{"coef": "mzv_4", "key": []}],
        }),
        json!({
            "op": "break_up_contour",
            "vars": ["x"],
            "wl": [{"coef": "mzv_4", "word": []}],
            "on_axis": [],
        }),
        json!({
            "op": "integration_step",
            "var": "x",
            "vars": ["x"],
            "parallel": false,
            "wordlist": [{"coef": "mzv_4/(1+x)^2", "shuffle": []}],
        }),
        json!({
            "op": "hyperflint",
            "vars": ["x", "y"],
            "vars_int": ["x"],
            "parallel": false,
            "wordlist": [{"coef": "mzv_4/(1+x)^2", "shuffle": []}],
        }),
        json!({
            "op": "sym_reduce",
            "vars": ["x"],
            "a": [{"prefactor": "mzv_4"}],
        }),
    ];
    for request in nested_mzv_requests {
        let response = eval(&request);
        let variables = string_array(&response["vars"]);
        assert!(variables.contains(&"mzv_4".to_owned()), "{response}");
        assert!(variables.len() < 20, "context was widened: {response}");
    }

    let integrated_parallel = eval(&json!({
        "op": "hyperflint",
        "vars": ["x", "y"],
        "vars_int": ["x", "y"],
        "f": "1/((1+x)^2*(1+y)^2)",
        "parallel": true,
        "check_divergences": true,
    }));
    assert_eq!(integrated_parallel["failed"], integrated["failed"]);
    assert_eq!(integrated_parallel["divergent"], integrated["divergent"]);
    assert_eq!(integrated_parallel["result"], integrated["result"]);
}
