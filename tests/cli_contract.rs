//! Command-line behavior that sits outside the mathematical JSON protocol.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(arguments: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hyperflint"))
        .args(arguments)
        .env("SYMBOLICA_HIDE_BANNER", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hyperflint");
    child
        .stdin
        .as_mut()
        .expect("piped stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    drop(child.stdin.take());
    child.wait_with_output().expect("wait for hyperflint")
}

#[test]
fn help_version_dispatch_and_factor_options_match_the_upstream_cli() {
    for option in ["--help", "-h"] {
        let output = run(&[option], "");
        assert!(output.status.success(), "{option}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("eval-json"));
    }

    for option in ["--version", "-v"] {
        let output = run(&[option], "");
        assert!(output.status.success(), "{option}");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("HF_VERSION: 0.1.0.0"), "{stdout}");
        assert!(
            stdout.contains("HF_BUILD_VARIANT: rust-symbolica"),
            "{stdout}"
        );
    }

    let missing_op = run(&["eval-json"], r#"{"a":"x"}"#);
    assert_eq!(missing_op.status.code(), Some(1));
    assert!(missing_op.stdout.is_empty());
    assert!(String::from_utf8_lossy(&missing_op.stderr).contains("missing \"op\""));

    let unknown_op = run(&["eval-json"], r#"{"op":"does_not_exist"}"#);
    assert_eq!(unknown_op.status.code(), Some(2));
    let response: serde_json::Value = serde_json::from_slice(&unknown_op.stdout).unwrap();
    assert_eq!(response["op"], "does_not_exist");
    assert!(response["error"].as_str().unwrap().contains("unknown op"));

    let factor = run(&["factor", "x+y", "--vars", "y,x"], "");
    assert!(
        factor.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&factor.stdout),
        String::from_utf8_lossy(&factor.stderr)
    );

    let bad_option = run(&["factor", "x+y", "--unknown"], "");
    assert_eq!(bad_option.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&bad_option.stderr).contains("unknown option"));
}
