//! Licensed end-to-end regression for HyperFLINT's smallest public Smirnov input.
//!
//! The integration order is part of the upstream fixture contract: it is the
//! order used by HyperFLINT's benchmark and subprocess smoke gates, rather than
//! the lexical order of the five variables.

use hyperbolica::bridge;
use hyperbolica::core::{PolyCtx, Rat};
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
        &variables[..5],
        ["t1", "t2", "t3", "t4", "t5"],
        "the user-variable prefix drifted"
    );

    // Compare normalized rational functions rather than printer strings. This
    // keeps the gate sensitive to the period while tolerating harmless changes
    // in Symbolica's term ordering or parenthesization.
    let ctx = PolyCtx::new(variables).expect("response variables form a context");
    let actual = Rat::parse(
        ctx.clone(),
        result[0]["coef"]
            .as_str()
            .expect("a shuffle coefficient is a string"),
    )
    .expect("the terminal coefficient parses");
    let expected = Rat::parse(ctx, "1+mzv_3-4*mzv_2^2/5").unwrap();
    assert_eq!(actual, expected);
}
