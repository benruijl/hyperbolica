//! Transport-neutral implementation of HyperFLINT's JSON protocol.

mod algebra;
mod integration;
mod lr;
mod mzv_data;
mod narrow;
mod reduction;
mod series;
mod symcoef;
mod thread_budget;
mod wire;
mod words;

use serde_json::Value;

use crate::error::{Error, Result};

pub const SCHEMA_VERSION: u64 = 2;

/// Evaluate one JSON request and return a protocol response.
pub fn evaluate(request: &Value) -> Result<Value> {
    let op = wire::string_field(request, "op")?;

    // The pinned C++ compatibility handler applies
    // HF_MAX_THREADS_PER_CALL only to hyperflint_sym. Keep that exact scope,
    // while using a request-local Rayon pool instead of process-global state.
    if op == "hyperflint" {
        return thread_budget::with_max_threads_per_call(|force_serial| {
            evaluate_hyperflint(request, force_serial)
        });
    }

    if let Some(response) = words::evaluate(request, op) {
        return response;
    }
    if let Some(response) = symcoef::evaluate(request, op) {
        return response;
    }
    if let Some(response) = algebra::evaluate(request, op) {
        return response;
    }
    if let Some(response) = lr::evaluate(request, op) {
        return response;
    }
    if let Some(response) = integration::evaluate(request, op) {
        return response;
    }
    if let Some(response) = reduction::evaluate(request, op) {
        return response;
    }
    if let Some(response) = series::evaluate(request, op) {
        return response;
    }

    Err(Error::InvalidInput(format!("unknown op `{op}`")))
}

fn evaluate_hyperflint(request: &Value, force_serial: bool) -> Result<Value> {
    if force_serial {
        let mut serial_request = request.clone();
        serial_request
            .as_object_mut()
            .expect("a request with a string op is a JSON object")
            .insert("parallel".into(), Value::Bool(false));
        return integration::evaluate(&serial_request, "hyperflint")
            .expect("integration bridge recognizes hyperflint");
    }
    integration::evaluate(request, "hyperflint").expect("integration bridge recognizes hyperflint")
}

pub fn evaluate_json(input: &str) -> Result<String> {
    let request: Value = serde_json::from_str(input)?;
    Ok(serde_json::to_string(&evaluate(&request)?)?)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::core::{Poly, PolyCtx};

    #[test]
    fn basic_protocol_operation() {
        let output = evaluate_json(r#"{"op":"mul","a":"x+y","b":"x-y"}"#).unwrap();
        let value: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["op"], "mul");
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        assert_eq!(
            Poly::parse(ctx.clone(), value["result"].as_str().unwrap()).unwrap(),
            Poly::parse(ctx, "x^2-y^2").unwrap()
        );
    }

    #[test]
    fn resultant_uses_explicit_variable() {
        let request = json!({
            "op": "resultant",
            "a": "x^2+y*x+1",
            "b": "x-y",
            "var": "x",
            "vars": ["x", "y"]
        });
        let response = evaluate(&request).unwrap();
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        assert_eq!(
            Poly::parse(ctx.clone(), response["result"].as_str().unwrap()).unwrap(),
            Poly::parse(ctx, "2*y^2+1").unwrap()
        );
    }

    #[test]
    fn schema_gate_applies_only_to_enveloped_lr_operations() {
        let ordinary = evaluate(&json!({
            "op": "mul",
            "a": "x",
            "b": "x",
            "schema_version_min": SCHEMA_VERSION + 1,
        }))
        .unwrap();
        assert_eq!(ordinary["result"], "x^2");

        let gated = evaluate(&json!({
            "op": "find_lr_orders",
            "xvars": ["x"],
            "polys": ["x+1"],
            "schema_version_min": SCHEMA_VERSION + 1,
        }))
        .unwrap_err();
        assert!(gated.to_string().contains("schema_version_min"));
    }

    #[test]
    fn upstream_flat_json_defaults_are_preserved() {
        let discovered = evaluate(&json!({
            "op": "mul",
            "a": "x",
            "b": "y",
            "vars": [],
        }))
        .unwrap();
        assert_eq!(discovered["vars"], json!(["x", "y"]));

        let zero_power = evaluate(&json!({"op": "pow", "a": "x+1"})).unwrap();
        assert_eq!(zero_power["result"], "1");

        let mistyped_zero_power = evaluate(&json!({"op": "pow", "a": "x+1", "n": "2"})).unwrap();
        assert_eq!(mistyped_zero_power["result"], "1");

        let empty_shuffle = evaluate(&json!({"op": "shuffle_words"})).unwrap();
        assert_eq!(empty_shuffle["vars"], json!(["x"]));
        assert_eq!(empty_shuffle["result"], json!([]));

        let ignored_contour_metadata = evaluate(&json!({
            "op": "break_up_contour",
            "wl": [],
            "on_axis": [{"not": "interpreted"}],
            "vars": ["x"],
        }))
        .unwrap();
        assert_eq!(ignored_contour_metadata["result"], json!([]));
    }

    #[test]
    fn serial_thread_budget_overrides_parallel_request_without_mutating_it() {
        let request = json!({
            "op": "hyperflint",
            "vars": ["x"],
            "vars_int": ["x"],
            "f": "1/(x+1)^2",
            "parallel": true,
            "check_divergences": true,
        });
        let response = thread_budget::with_effective_thread_limit(1, |force_serial| {
            assert!(force_serial);
            evaluate_hyperflint(&request, force_serial)
        })
        .unwrap();

        assert_ne!(response["failed"], true);
        assert_eq!(response["result"], json!([{"coef": "1", "key": []}]));
        assert_eq!(request["parallel"], true);
    }
}
