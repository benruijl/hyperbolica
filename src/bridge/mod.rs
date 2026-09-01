//! Transport-neutral implementation of HyperFLINT's JSON protocol.

mod algebra;
mod integration;
mod lr;
mod mzv_data;
mod narrow;
mod reduction;
mod series;
mod symcoef;
mod wire;
mod words;

use serde_json::Value;

use crate::error::{Error, Result};

pub const SCHEMA_VERSION: u64 = 2;

/// Evaluate one JSON request and return a protocol response.
pub fn evaluate(request: &Value) -> Result<Value> {
    let op = wire::string_field(request, "op")?;
    if request
        .get("schema_version_min")
        .and_then(Value::as_u64)
        .is_some_and(|minimum| minimum > SCHEMA_VERSION)
    {
        return Err(Error::InvalidInput(format!(
            "schema_version_min exceeds supported schema version {SCHEMA_VERSION}"
        )));
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
}
