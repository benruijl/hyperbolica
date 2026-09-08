//! Licensed end-to-end regression for HyperFLINT's smallest public Smirnov input.
//!
//! The integration order is part of the upstream fixture contract: it is the
//! order used by HyperFLINT's benchmark and subprocess smoke gates, rather than
//! the lexical order of the five variables.

use hyperbolica::bridge;
use hyperbolica::core::{PolyCtx, Rat};
use hyperbolica::reduce::mzv_expression_atom;
use hyperbolica::symbols::mzv_atom;
use serde_json::json;

#[test]
fn smirnov_tst0_runs_through_the_complete_hyperflint_pipeline() {
    // The pinned C++ smoke gate embeds this fixture without layout whitespace;
    // reproduce that wire payload exactly (the corpus test separately pins the
    // byte-identical source file, including its line breaks).
    let integrand = include_str!("data/smirnov/tst0.txt")
        .split_whitespace()
        .collect::<String>();
    let response = bridge::evaluate(&json!({
        "op": "hyperflint",
        "f": integrand,
        "vars_int": ["t4", "t5", "t1", "t2", "t3"],
        "vars": ["t1", "t2", "t3", "t4", "t5"],
    }))
    .expect("the Smirnov request is valid");

    assert_eq!(response["op"], "hyperflint");
    assert_ne!(response["failed"], true, "{response}");
    assert_ne!(response["divergent"], true, "{response}");
    assert!(response["timing_compute_s"].as_f64().is_some());

    let result = response["result"]
        .as_array()
        .expect("a successful integration returns a shuffle list");
    assert_eq!(
        result.len(),
        1,
        "unexpected terminal shuffle list: {response}"
    );
    assert_eq!(result[0]["key"], json!([]));

    let variables = response["vars"]
        .as_array()
        .expect("the response reports its polynomial context")
        .iter()
        .map(|variable| {
            variable
                .as_str()
                .expect("context variables are strings")
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        variables,
        ["t1", "t2", "t3", "t4", "t5"],
        "period constants must not inflate the integration context"
    );

    // Compare normalized rational functions rather than printer strings. This
    // keeps the gate sensitive to the period while tolerating harmless changes
    // in Symbolica's term ordering or parenthesization.
    // Period tuples deliberately do not add constants to response.vars. Use
    // an independent oracle ring containing the expected MZV generators.
    let ctx = PolyCtx::from_indeterminates([mzv_atom(&[2]), mzv_atom(&[3])]).unwrap();
    let actual_atom = mzv_expression_atom(
        result[0]["coef"]
            .as_str()
            .expect("a shuffle coefficient is a string"),
    )
    .unwrap();
    let actual = Rat::from_atom(ctx.clone(), actual_atom.as_view())
        .expect("the terminal coefficient parses");
    let expected_atom = mzv_expression_atom("1+mzv_3-4*mzv_2^2/5").unwrap();
    let expected = Rat::from_atom(ctx, expected_atom.as_view()).unwrap();
    assert_eq!(actual, expected);
}
