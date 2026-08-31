use std::io::Read;

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("hyperflint: {message}");
    std::process::exit(2);
}

fn main() {
    // Symbolica's restricted-mode informational banner is useful interactively
    // but would corrupt the machine-readable eval-json stdout contract.
    if std::env::var_os("SYMBOLICA_HIDE_BANNER").is_none() {
        // SAFETY: this happens before Symbolica or any worker thread is started.
        unsafe { std::env::set_var("SYMBOLICA_HIDE_BANNER", "1") };
    }
    let mut arguments = std::env::args().skip(1);
    match arguments.next().as_deref() {
        Some("--version" | "-V") => {
            println!("hyperflint {} (Rust/Symbolica)", env!("CARGO_PKG_VERSION"));
        }
        Some("eval-json") => {
            let mut input = String::new();
            std::io::stdin()
                .read_to_string(&mut input)
                .unwrap_or_else(|error| fail(error));
            match hyperbolica::bridge::evaluate_json(&input) {
                Ok(response) => println!("{response}"),
                Err(error) => {
                    let op = serde_json::from_str::<serde_json::Value>(&input)
                        .ok()
                        .and_then(|request| request.get("op")?.as_str().map(ToOwned::to_owned))
                        .unwrap_or_default();
                    println!(
                        "{}",
                        serde_json::json!({"op": op, "error": error.to_string()})
                    );
                    std::process::exit(1);
                }
            }
        }
        Some("factor") => {
            let expression = arguments
                .next()
                .unwrap_or_else(|| fail("factor requires an expression"));
            let request = serde_json::json!({"op": "factor", "expr": expression});
            let response =
                hyperbolica::bridge::evaluate(&request).unwrap_or_else(|error| fail(error));
            print!("{}", response["constant"].as_str().unwrap_or("1"));
            if let Some(factors) = response["factors"].as_array() {
                for factor in factors {
                    let base = factor[0].as_str().unwrap_or_default();
                    let exponent = factor[1].as_u64().unwrap_or(1);
                    if exponent == 1 {
                        print!(" * ({base})");
                    } else {
                        print!(" * ({base})^{exponent}");
                    }
                }
            }
            println!();
        }
        _ => fail("usage: hyperflint {eval-json|factor <expression>|--version}"),
    }
}
