use std::io::Read;

const BUILD_VARIANT: &str = "rust-symbolica";

fn exit_with(message: impl std::fmt::Display, status: i32) -> ! {
    eprintln!("hyperflint: {message}");
    std::process::exit(status);
}

fn usage() {
    eprintln!(
        "HyperFLINT CLI (Rust/Symbolica)\n\
         \n\
           hyperflint eval-json < request.json\n\
           hyperflint factor <expression> [--vars x,y,...]\n\
           hyperflint --version\n\
         \n\
         Supported core ops include factor, add/sub/mul, neg, pow,\n\
         partial_fractions, linear_factors, LR search, and integration."
    );
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
        Some("--help" | "-h") => usage(),
        Some("--version" | "-v" | "-V") => {
            println!(
                "HF_VERSION: {}.0\nHF_BUILD_VARIANT: {BUILD_VARIANT}",
                env!("CARGO_PKG_VERSION")
            );
        }
        Some("eval-json") => {
            let mut input = String::new();
            std::io::stdin()
                .read_to_string(&mut input)
                .unwrap_or_else(|error| exit_with(error, 1));
            let request = serde_json::from_str::<serde_json::Value>(&input)
                .unwrap_or_else(|_| exit_with("eval-json: missing \"op\"", 1));
            let op = request
                .get("op")
                .and_then(serde_json::Value::as_str)
                .filter(|op| !op.is_empty())
                .unwrap_or_else(|| exit_with("eval-json: missing \"op\"", 1));
            match hyperbolica::bridge::evaluate_json(&input) {
                Ok(response) => println!("{response}"),
                Err(error) => {
                    println!(
                        "{}",
                        serde_json::json!({"op": op, "error": error.to_string()})
                    );
                    std::process::exit(2);
                }
            }
        }
        Some("factor") => {
            let expression = arguments
                .next()
                .unwrap_or_else(|| exit_with("factor: expected <expression>", 1));
            let mut variables = None;
            while let Some(option) = arguments.next() {
                match option.as_str() {
                    "--vars" => {
                        let value = arguments
                            .next()
                            .unwrap_or_else(|| exit_with("factor: --vars needs a value", 1));
                        let parsed = value
                            .split(',')
                            .filter(|name| !name.is_empty())
                            .map(ToOwned::to_owned)
                            .collect::<Vec<_>>();
                        if !parsed.is_empty() {
                            variables = Some(parsed);
                        }
                    }
                    other => exit_with(format!("factor: unknown option `{other}`"), 1),
                }
            }
            let mut request = serde_json::json!({"op": "factor", "expr": expression});
            if let Some(variables) = variables {
                request["vars"] = serde_json::json!(variables);
            }
            let response =
                hyperbolica::bridge::evaluate(&request).unwrap_or_else(|error| exit_with(error, 2));
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
        Some(command) => {
            eprintln!("hyperflint: unknown command `{command}`");
            usage();
            std::process::exit(1);
        }
        None => {
            usage();
            std::process::exit(1);
        }
    }
}
