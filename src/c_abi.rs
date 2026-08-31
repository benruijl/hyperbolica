//! Stable C ABI for transport-neutral JSON operations.

use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use serde_json::{Map, Value, json};

use crate::bridge::{SCHEMA_VERSION, evaluate};

const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), ".0");
static VERSION_C: &[u8] = concat!(env!("CARGO_PKG_VERSION"), ".0\0").as_bytes();

fn envelope(op: &str, body: Value) -> Value {
    let mut output = Map::new();
    output.insert("op".into(), Value::String(op.into()));
    output.insert("schema_version".into(), Value::from(SCHEMA_VERSION));
    output.insert("hf_version".into(), Value::String(VERSION.into()));
    if let Value::Object(body) = body {
        for (key, value) in body {
            if !matches!(key.as_str(), "op" | "schema_version" | "hf_version") {
                output.insert(key, value);
            }
        }
    }
    Value::Object(output)
}

fn error_envelope(op: &str, message: impl ToString) -> Value {
    envelope(op, json!({"error": message.to_string()}))
}

fn owned_json(value: &Value) -> *mut c_char {
    let Ok(serialized) = serde_json::to_string(value) else {
        return ptr::null_mut();
    };
    CString::new(serialized)
        .map(CString::into_raw)
        .unwrap_or(ptr::null_mut())
}

fn dispatch(request_json: *const c_char, op: &str) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if request_json.is_null() {
            return error_envelope(op, "request_json is NULL");
        }
        // SAFETY: the C contract requires a readable NUL-terminated input for
        // the duration of this call; no pointer is retained.
        let input = unsafe { CStr::from_ptr(request_json) };
        let input = match input.to_str() {
            Ok(input) => input,
            Err(error) => return error_envelope(op, format!("request is not UTF-8: {error}")),
        };
        let mut request: Value = match serde_json::from_str(input) {
            Ok(request) => request,
            Err(error) => return error_envelope(op, error),
        };
        let Some(object) = request.as_object_mut() else {
            return error_envelope(op, "request must be a JSON object");
        };
        object.insert("op".into(), Value::String(op.into()));
        match evaluate(&request) {
            Ok(response) => envelope(op, response),
            Err(error) => error_envelope(op, error),
        }
    }));

    match result {
        Ok(value) => owned_json(&value),
        Err(_) => owned_json(&error_envelope(op, "panic contained at C ABI boundary")),
    }
}

/// Release a string returned by any `hf_*` operation. A null pointer is a no-op.
///
/// # Safety
///
/// `value` must be null or a pointer returned by this library that has not
/// already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hf_free_string(value: *mut c_char) {
    if !value.is_null() {
        // SAFETY: guaranteed by the public ownership contract above.
        drop(unsafe { CString::from_raw(value) });
    }
}

macro_rules! operation {
    ($function:ident, $op:literal) => {
        #[doc = concat!("Evaluate the `", $op, "` JSON operation.")]
        #[unsafe(no_mangle)]
        pub extern "C" fn $function(request_json: *const c_char) -> *mut c_char {
            dispatch(request_json, $op)
        }
    };
}

operation!(hf_partial_fractions, "partial_fractions");
operation!(hf_linear_factors, "linear_factors");
operation!(hf_find_lr_orders, "find_lr_orders");
operation!(hf_factor_table, "factor_table");
operation!(hf_find_lr_orders_scan, "find_lr_orders_scan");
operation!(hf_hyperflint_sym, "hyperflint");

/// Static-lifetime build version; callers must not free this pointer.
#[unsafe(no_mangle)]
pub extern "C" fn hf_version_string() -> *const c_char {
    VERSION_C.as_ptr().cast()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_request_is_an_owned_error_envelope() {
        let output = hf_linear_factors(ptr::null());
        assert!(!output.is_null());
        // SAFETY: `output` came from this module and remains live here.
        let text = unsafe { CStr::from_ptr(output) }.to_str().unwrap();
        let value: Value = serde_json::from_str(text).unwrap();
        assert_eq!(value["op"], "linear_factors");
        assert_eq!(value["schema_version"], SCHEMA_VERSION);
        assert!(value["error"].as_str().unwrap().contains("NULL"));
        // SAFETY: exactly one matching free for the returned pointer.
        unsafe { hf_free_string(output) };
    }
}
