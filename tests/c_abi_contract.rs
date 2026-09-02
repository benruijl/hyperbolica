//! Contract tests for the stable C ABI.
//!
//! The malformed-input cases remain CAS-independent. The success cases mirror
//! the small upstream C-ABI fixtures and cross the real Symbolica-backed
//! mathematical dispatch before checking payloads and releasing every result.

use std::ffi::{CStr, CString, c_char};
use std::ptr;

use hyperbolica::bridge::SCHEMA_VERSION;
use hyperbolica::c_abi::{
    hf_factor_table, hf_find_lr_orders, hf_find_lr_orders_scan, hf_free_string, hf_hyperflint_sym,
    hf_linear_factors, hf_partial_fractions, hf_version_string,
};
use serde_json::{Value, json};

type Operation = extern "C" fn(*const c_char) -> *mut c_char;

const OPERATIONS: [(&str, Operation); 6] = [
    ("partial_fractions", hf_partial_fractions),
    ("linear_factors", hf_linear_factors),
    ("find_lr_orders", hf_find_lr_orders),
    ("factor_table", hf_factor_table),
    ("find_lr_orders_scan", hf_find_lr_orders_scan),
    ("hyperflint", hf_hyperflint_sym),
];

const ABI_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), ".0");

#[derive(Debug)]
struct AbiResponse {
    bytes: String,
    value: Value,
}

unsafe fn take_response(pointer: *mut c_char) -> AbiResponse {
    assert!(!pointer.is_null(), "ordinary ABI errors must be allocated");
    // SAFETY: every operation returns a live NUL-terminated allocation.
    let bytes = unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .expect("ABI responses are UTF-8")
        .to_owned();
    let value = serde_json::from_str(&bytes).expect("ABI responses are JSON");
    // SAFETY: this is the one matching release for the operation result.
    unsafe { hf_free_string(pointer) };
    AbiResponse { bytes, value }
}

unsafe fn take_json(pointer: *mut c_char) -> Value {
    // SAFETY: forwarded from this helper's caller.
    unsafe { take_response(pointer) }.value
}

fn encoded_request(request: &Value) -> CString {
    CString::new(serde_json::to_vec(request).expect("test request is JSON"))
        .expect("JSON serialization contains no NUL bytes")
}

fn invoke(operation: Operation, request: &Value) -> AbiResponse {
    let request = encoded_request(request);
    let pointer = operation(request.as_ptr());
    assert!(
        !pointer.is_null(),
        "valid requests must return an allocation"
    );
    // SAFETY: `pointer` is a live operation result and is consumed once.
    unsafe { take_response(pointer) }
}

fn invoke_owned_pair(operation: Operation, request: &Value) -> [AbiResponse; 2] {
    let request = encoded_request(request);
    let first = operation(request.as_ptr());
    let second = operation(request.as_ptr());
    assert!(!first.is_null());
    assert!(!second.is_null());
    assert_ne!(
        first, second,
        "simultaneously live success results must be independent allocations"
    );

    // The public `char *` result is caller-owned writable storage. Restore the
    // byte so both payloads can still be compared before their matching frees.
    // SAFETY: `first` is a nonempty JSON allocation which remains live.
    unsafe {
        let byte = *first.cast::<u8>();
        *first.cast::<u8>() = b'#';
        *first.cast::<u8>() = byte;
    }

    // SAFETY: both pointers are distinct, live operation results and each is
    // consumed by exactly one matching `hf_free_string` call.
    let first = unsafe { take_response(first) };
    let second = unsafe { take_response(second) };
    [first, second]
}

fn assert_error_envelope(value: &Value, op: &str, message_fragment: &str) {
    assert_eq!(value["op"], op);
    assert_eq!(value["schema_version"], SCHEMA_VERSION);
    assert_eq!(
        value["hf_version"],
        concat!(env!("CARGO_PKG_VERSION"), ".0")
    );
    assert!(
        value["error"]
            .as_str()
            .is_some_and(|message| message.contains(message_fragment)),
        "unexpected error envelope: {value}"
    );
}

fn assert_success_envelope(response: &AbiResponse, op: &str) {
    assert_eq!(response.value["op"], op);
    assert_eq!(response.value["schema_version"], SCHEMA_VERSION);
    assert_eq!(response.value["hf_version"], ABI_VERSION);
    assert!(response.value.get("error").is_none(), "{response:?}");

    let prefix = format!(
        "{{\"op\":\"{op}\",\"schema_version\":{SCHEMA_VERSION},\"hf_version\":\"{ABI_VERSION}\""
    );
    assert!(
        response.bytes.starts_with(&prefix),
        "envelope fields must lead the byte payload: {}",
        response.bytes
    );
    assert_eq!(response.bytes.matches("\"schema_version\":").count(), 1);
    assert_eq!(response.bytes.matches("\"hf_version\":").count(), 1);
}

#[test]
fn every_operation_returns_an_owned_null_input_error() {
    for (op, operation) in OPERATIONS {
        let first = operation(ptr::null());
        let second = operation(ptr::null());
        assert!(!first.is_null());
        assert!(!second.is_null());
        assert_ne!(
            first, second,
            "operation results must be independent allocations"
        );

        // The public return type promises writable caller-owned storage.
        // SAFETY: `first` is non-null and points at a nonempty JSON string.
        unsafe {
            let byte = *first.cast::<u8>();
            *first.cast::<u8>() = b'#';
            *first.cast::<u8>() = byte;
        }

        // SAFETY: both pointers are distinct, live operation results.
        let first = unsafe { take_json(first) };
        let second = unsafe { take_json(second) };
        assert_error_envelope(&first, op, "NULL");
        assert_eq!(first, second);
    }

    // SAFETY: explicitly guaranteed by the public lifecycle contract.
    unsafe { hf_free_string(ptr::null_mut()) };
}

#[test]
fn malformed_transport_inputs_are_contained_before_cas_dispatch() {
    let invalid_utf8 = [0xff_u8, 0];
    let invalid_json = b"{not-json}\0";
    let non_object = b"[]\0";

    let cases = [
        (invalid_utf8.as_ptr().cast(), "not UTF-8"),
        (invalid_json.as_ptr().cast(), "line 1"),
        (non_object.as_ptr().cast(), "JSON object"),
    ];
    for (input, expected) in cases {
        // SAFETY: each input is a live NUL-terminated byte sequence for the call.
        let value = unsafe { take_json(hf_partial_fractions(input)) };
        assert_error_envelope(&value, "partial_fractions", expected);
    }
}

#[test]
fn version_pointer_is_static_utf8_and_has_four_numeric_components() {
    let first = hf_version_string();
    let second = hf_version_string();
    assert!(!first.is_null());
    assert_eq!(first, second, "borrowed version pointer must be stable");

    // SAFETY: `hf_version_string` returns a process-lifetime C string.
    let version = unsafe { CStr::from_ptr(first) }.to_str().unwrap();
    assert_eq!(version, concat!(env!("CARGO_PKG_VERSION"), ".0"));
    let components = version.split('.').collect::<Vec<_>>();
    assert_eq!(components.len(), 4);
    assert!(
        components
            .iter()
            .all(|component| !component.is_empty() && component.parse::<u64>().is_ok())
    );
}

#[test]
fn successful_math_operations_cross_the_abi_with_owned_envelopes() {
    // Mirrors upstream test_c_abi_pfrac_cli_snapshot.cpp, widened to a
    // repeated pole so the payload pins coefficient ordering as well.
    let partial_fraction_request = json!({
        "op": "partial_fractions",
        "f": "(2*x+3)/((x-1)^2*(x+1))",
        "var": "x",
        "vars": ["x"],
    });
    let [partial_fractions, repeated] =
        invoke_owned_pair(hf_partial_fractions, &partial_fraction_request);
    assert_success_envelope(&partial_fractions, "partial_fractions");
    assert_eq!(partial_fractions.bytes, repeated.bytes);
    assert_eq!(
        partial_fractions.bytes,
        serde_json::to_string(&json!({
            "op": "partial_fractions",
            "schema_version": SCHEMA_VERSION,
            "hf_version": ABI_VERSION,
            "var": "x",
            "polynomial_part": "0",
            "poles": [
                {"pole": "-1", "multiplicity": 1, "coefs": ["1/4"]},
                {"pole": "1", "multiplicity": 2, "coefs": ["-1/4", "5/2"]},
            ],
            "vars": ["x"],
        }))
        .unwrap()
    );

    // Exact upstream linear-factor snapshot fixture: (x-1)(x+1).
    let linear_factors = invoke(
        hf_linear_factors,
        &json!({
            "op": "linear_factors",
            "poly": "x^2 - 1",
            "var": "x",
            "vars": ["x"],
        }),
    );
    assert_success_envelope(&linear_factors, "linear_factors");
    assert_eq!(
        linear_factors.bytes,
        serde_json::to_string(&json!({
            "op": "linear_factors",
            "schema_version": SCHEMA_VERSION,
            "hf_version": ABI_VERSION,
            "constant": "1",
            "linear": [[1, "-1", "1"], [1, "1", "1"]],
            "nonlinear": [],
            "vars": ["x"],
        }))
        .unwrap()
    );

    // Exact upstream find_lr_orders C-ABI snapshot fixture. Only the timing
    // field varies, so pin every mathematical decision independently.
    let lr = invoke(
        hf_find_lr_orders,
        &json!({
            "op": "find_lr_orders",
            "xvars": ["x"],
            "polys": ["x"],
        }),
    );
    assert_success_envelope(&lr, "find_lr_orders");
    assert_eq!(lr.value["best_order"], json!(["x"]));
    assert_eq!(lr.value["score"], json!(1.0));
    assert_eq!(lr.value["nolr"], false);
    assert_eq!(lr.value["strategy"], "LR_NoOpt");
    assert_eq!(lr.value["nXVars"], 1);
    assert_eq!(lr.value["nGroups"], 1);
    assert_eq!(lr.value["nPolys"], json!([1]));
    assert!(
        lr.value["timing_compute_s"]
            .as_f64()
            .is_some_and(|seconds| seconds.is_finite() && seconds >= 0.0)
    );

    // Exercise the two remaining exported LR helpers through successful,
    // independently-owned calls as well. Their timing fields are deliberately
    // not byte-stable, so compare the mathematical payloads instead.
    let factor_table_request = json!({
        "op": "factor_table",
        "xvars": ["x"],
        "groups": [["x"]],
        "order": ["x"],
    });
    let [factor_table, factor_table_repeated] =
        invoke_owned_pair(hf_factor_table, &factor_table_request);
    for response in [&factor_table, &factor_table_repeated] {
        assert_success_envelope(response, "factor_table");
        assert_eq!(response.value["order"], json!(["x"]));
        assert_eq!(response.value["polys"], json!(["x"]));
        assert_eq!(response.value["pairs"], json!([]));
        assert_eq!(response.value["stats"]["pairs_total"], 0);
    }
    assert_eq!(
        factor_table.value["singletons"],
        factor_table_repeated.value["singletons"]
    );

    let scan_request = json!({
        "op": "find_lr_orders_scan",
        "xvars": ["x"],
        "groups": [["x"]],
        "exps": [[[-1, 0]]],
    });
    let [scan, scan_repeated] = invoke_owned_pair(hf_find_lr_orders_scan, &scan_request);
    for response in [&scan, &scan_repeated] {
        assert_success_envelope(response, "find_lr_orders_scan");
        assert_eq!(response.value["nXVars"], 1);
        assert_eq!(response.value["nGroups"], 1);
        assert!(response.value["orders"].is_array());
        assert!(
            response.value["timing_compute_s"]
                .as_f64()
                .is_some_and(|seconds| seconds.is_finite() && seconds >= 0.0)
        );
    }
    assert_eq!(scan.value["projective"], scan_repeated.value["projective"]);
    assert_eq!(scan.value["truncated"], scan_repeated.value["truncated"]);
    assert_eq!(scan.value["orders"], scan_repeated.value["orders"]);

    // A convergent integral over [0, infinity]: integral dx/(1+x)^2 = 1.
    // This keeps the full exported driver live while remaining millisecond
    // scale and deterministic with parallel execution disabled.
    let integration = invoke(
        hf_hyperflint_sym,
        &json!({
            "op": "hyperflint",
            "vars": ["x"],
            "vars_int": ["x"],
            "f": "1/(1+x)^2",
            "parallel": false,
            "check_divergences": true,
        }),
    );
    assert_success_envelope(&integration, "hyperflint");
    assert!(integration.value.get("failed").is_none(), "{integration:?}");
    assert!(
        integration.value.get("divergent").is_none(),
        "{integration:?}"
    );
    assert_eq!(
        integration.value["result"],
        json!([{"coef": "1", "key": []}])
    );
    assert!(integration.value.get("algebraic_letters").is_none());
    assert_eq!(integration.value["vars"][0], "x");
    assert!(
        integration.value["timing_compute_s"]
            .as_f64()
            .is_some_and(|seconds| seconds.is_finite() && seconds >= 0.0)
    );
}
