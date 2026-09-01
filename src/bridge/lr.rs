//! Linear-reducibility search, scan, and factor-table protocol operations.

mod factor_table;
mod input;
mod search;
mod verify;

#[cfg(test)]
mod tests;

use serde_json::Value;

use crate::error::{Error, Result};

pub(super) fn evaluate(request: &Value, op: &str) -> Option<Result<Value>> {
    matches!(
        op,
        "find_lr_orders" | "find_lr_orders_scan" | "factor_table"
    )
    .then(|| evaluate_supported(request, op))
}

fn evaluate_supported(request: &Value, op: &str) -> Result<Value> {
    if request
        .get("schema_version_min")
        .and_then(Value::as_u64)
        .is_some_and(|minimum| minimum > super::SCHEMA_VERSION)
    {
        return Err(Error::InvalidInput(format!(
            "schema_version_min exceeds supported schema version {}",
            super::SCHEMA_VERSION
        )));
    }

    match op {
        "find_lr_orders" => search::evaluate_find_orders(request, op),
        "find_lr_orders_scan" => search::evaluate_scan(request, op),
        "factor_table" => factor_table::evaluate(request, op),
        _ => unreachable!("operation was checked by lr::evaluate"),
    }
}
