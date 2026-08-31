//! CAS-independent contract tests for the stable C ABI.
//!
//! These cases deliberately stop before dispatching a valid mathematical
//! request, so they exercise transport, error, ownership, and version behavior
//! without acquiring a Symbolica runtime.

use std::ffi::{CStr, c_char};
use std::ptr;

use hyperbolica::bridge::SCHEMA_VERSION;
use hyperbolica::c_abi::{
    hf_factor_table, hf_find_lr_orders, hf_find_lr_orders_scan, hf_free_string, hf_hyperflint_sym,
    hf_linear_factors, hf_partial_fractions, hf_version_string,
};
use serde_json::Value;

type Operation = extern "C" fn(*const c_char) -> *mut c_char;

const OPERATIONS: [(&str, Operation); 6] = [
    ("partial_fractions", hf_partial_fractions),
    ("linear_factors", hf_linear_factors),
    ("find_lr_orders", hf_find_lr_orders),
    ("factor_table", hf_factor_table),
    ("find_lr_orders_scan", hf_find_lr_orders_scan),
    ("hyperflint", hf_hyperflint_sym),
];

unsafe fn take_json(pointer: *mut c_char) -> Value {
    assert!(!pointer.is_null(), "ordinary ABI errors must be allocated");
    // SAFETY: every operation returns a live NUL-terminated allocation.
    let json = unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .expect("ABI responses are UTF-8");
    let value = serde_json::from_str(json).expect("ABI responses are JSON");
    // SAFETY: this is the one matching release for the operation result.
    unsafe { hf_free_string(pointer) };
    value
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
