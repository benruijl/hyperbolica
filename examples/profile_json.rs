//! Repeat one JSON request in-process for sampling-profiler runs.
//!
//! This intentionally measures a warm process. The locked benchmark harness
//! remains authoritative for cold-process Rust/HyperFLINT comparisons.

use std::io::Read;
use std::time::Instant;

fn comparable_response(response: &str) -> serde_json::Value {
    let mut value: serde_json::Value =
        serde_json::from_str(response).expect("bridge response must be JSON");
    assert!(
        value.get("error").is_none(),
        "bridge returned an error: {value}"
    );
    assert_ne!(
        value.get("failed"),
        Some(&serde_json::Value::Bool(true)),
        "bridge request failed: {value}"
    );
    // End-to-end integrations report elapsed compute time. Keep that in the
    // printed response, but exclude it from the determinism assertion.
    value
        .as_object_mut()
        .expect("bridge response must be an object")
        .remove("timing_compute_s");
    value
}

fn main() {
    let iterations = std::env::args()
        .nth(1)
        .map(|value| value.parse::<usize>())
        .transpose()
        .unwrap_or_else(|error| panic!("invalid iteration count: {error}"))
        .unwrap_or(1);
    assert!(iterations > 0, "iteration count must be positive");

    let mut request = String::new();
    std::io::stdin()
        .read_to_string(&mut request)
        .expect("failed to read the JSON request from stdin");

    let started = Instant::now();
    let first = hyperbolica::bridge::evaluate_json(&request)
        .unwrap_or_else(|error| panic!("request failed: {error}"));
    let expected = comparable_response(&first);
    for _ in 1..iterations {
        let response = hyperbolica::bridge::evaluate_json(&request)
            .unwrap_or_else(|error| panic!("request failed: {error}"));
        assert_eq!(
            comparable_response(&response),
            expected,
            "request produced a nondeterministic response"
        );
    }

    let elapsed = started.elapsed();
    eprintln!(
        "profile_json: {iterations} iteration(s), {:.6} s total, {:.6} s/iteration",
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64() / iterations as f64
    );
    println!("{first}");
}

#[cfg(test)]
mod tests {
    use super::comparable_response;

    #[test]
    fn timing_changes_do_not_hide_result_changes() {
        let a = comparable_response(r#"{"result":1,"timing_compute_s":0.1}"#);
        let b = comparable_response(r#"{"result":1,"timing_compute_s":0.2}"#);
        let c = comparable_response(r#"{"result":2,"timing_compute_s":0.1}"#);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
