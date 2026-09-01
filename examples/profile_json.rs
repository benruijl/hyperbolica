//! Repeat one JSON request in-process for sampling-profiler runs.
//!
//! This intentionally measures a warm process. The locked benchmark harness
//! remains authoritative for cold-process Rust/HyperFLINT comparisons.

use std::io::Read;
use std::time::Instant;

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
    for _ in 1..iterations {
        let response = hyperbolica::bridge::evaluate_json(&request)
            .unwrap_or_else(|error| panic!("request failed: {error}"));
        assert_eq!(
            response, first,
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
