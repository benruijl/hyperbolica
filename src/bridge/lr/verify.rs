use serde_json::Value;

use super::super::wire::wire_poly;
use crate::integrator::lr_verify::OrderVerifyResult;

pub(super) fn append_verify_result(response: &mut Value, result: &OrderVerifyResult) {
    response["order_is_lr"] = Value::Bool(result.is_lr);
    response["verify_malformed"] = Value::Bool(result.malformed);
    response["verify_blocking_step"] = Value::from(result.blocking_step);
    response["verify_blocking_degree"] = Value::from(result.blocking_degree);
    response["verify_forbidden_dep"] = Value::Bool(result.forbidden_dep);
    response["verify_blocking_letter"] = Value::String(
        result
            .blocking_letter
            .as_ref()
            .map(wire_poly)
            .unwrap_or_default(),
    );
}
