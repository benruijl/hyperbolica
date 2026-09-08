//! Exact transport regressions for deferred partial-fraction input.

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn evaluate(expression: &str, variables: &[String]) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hyperflint"))
        .arg("eval-json")
        .env("SYMBOLICA_HIDE_BANNER", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let request = json!({
        "op": "partial_fractions", "var": "x", "vars": variables, "f": expression,
    });
    let mut input = child.stdin.take().unwrap();
    serde_json::to_writer(&mut input, &request).unwrap();
    input.write_all(b"\n").unwrap();
    drop(input);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{request}: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value.get("error").is_none(), "{value}");
    assert_eq!(value["vars"], json!(variables));
    value
}

#[test]
fn deferred_bridge_preserves_exact_residues_fallbacks_and_large_powers() {
    // A rational parameter leading coefficient and one cancellation leave
    // exactly one double pole, without any convention-dependent expansion.
    let variables = ["x", "y", "z"].map(str::to_owned);
    let result = evaluate("(x+z)/(2*y*x+2*y*z)^3", &variables);
    assert_eq!(result["polynomial_part"], "0");
    assert_eq!(
        result["poles"],
        json!([{"pole": "-z", "multiplicity": 2, "coefs": ["0", "1/(8*y^3)"]}])
    );

    let variables = ["x".to_owned()];
    let improper = evaluate("(x^3+1)/(x+1)^2", &variables);
    assert_eq!(improper["polynomial_part"], "x - 2");
    assert_eq!(
        improper["poles"],
        json!([{"pole": "-1", "multiplicity": 1, "coefs": ["3"]}])
    );

    let nonlinear_block = evaluate("1/(x^2-1)^2", &variables);
    assert_eq!(nonlinear_block["polynomial_part"], "0");
    let poles = nonlinear_block["poles"].as_array().unwrap();
    assert_eq!(poles.len(), 2);
    for (root, first) in [("-1", "1/4"), ("1", "-1/4")] {
        let pole = poles.iter().find(|pole| pole["pole"] == root).unwrap();
        assert_eq!(pole["multiplicity"], 2);
        assert_eq!(pole["coefs"], json!([first, "1/4"]));
    }

    // The fully expanded denominator has binomial(45, 13) terms. The public
    // JSON path must retain its small base all the way into the PF adapter.
    let mut variables = vec!["x".to_owned()];
    variables.extend((0..12).map(|index| format!("y{index}")));
    let expression = format!("1/({}+1)^32", variables.join("+"));
    let wide = evaluate(&expression, &variables);
    assert_eq!(wide["polynomial_part"], "0");
    let poles = wide["poles"].as_array().unwrap();
    assert_eq!(poles.len(), 1);
    assert_eq!(poles[0]["multiplicity"], 32);
    let coefficients = poles[0]["coefs"].as_array().unwrap();
    assert_eq!(coefficients.len(), 32);
    assert!(
        coefficients[..31]
            .iter()
            .all(|coefficient| coefficient == "0")
    );
    assert_eq!(coefficients[31], "1");
}

#[test]
fn native_fallback_preserves_reordered_context_and_sparse_pole_orders() {
    for variables in [["x", "y"], ["y", "x"]] {
        let variables = variables.map(str::to_owned);
        let simple = evaluate("x+1/(x+1)", &variables);
        assert_eq!(simple["polynomial_part"], "x");
        assert_eq!(
            simple["poles"],
            json!([{"pole": "-1", "multiplicity": 1, "coefs": ["1"]}])
        );
        let sparse = evaluate("1+1/(x+1)^3+1/(x+1)+1/(x+2)", &variables);
        assert_eq!(sparse["polynomial_part"], "1");
        let poles = sparse["poles"].as_array().unwrap();
        assert_eq!(poles.len(), 2);
        let first = poles.iter().find(|pole| pole["pole"] == "-1").unwrap();
        assert_eq!(first["multiplicity"], 3);
        assert_eq!(first["coefs"], json!(["1", "0", "1"]));
        let second = poles.iter().find(|pole| pole["pole"] == "-2").unwrap();
        assert_eq!(second["multiplicity"], 1);
        assert_eq!(second["coefs"], json!(["1"]));
    }
}
