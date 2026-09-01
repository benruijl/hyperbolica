use std::env;

fn main() {
    const NAME: &str = "HYPERBOLICA_SUBTROPICA_VERSION";
    println!("cargo:rerun-if-env-changed={NAME}");

    let version = env::var(NAME).unwrap_or_default();
    if !version.is_empty() {
        assert!(
            version.trim() == version && !version.chars().any(char::is_control),
            "{NAME} must be a nonempty printable string without surrounding whitespace"
        );
    }
    println!("cargo:rustc-env=HYPERBOLICA_LIBRARYLINK_VERSION_OVERRIDE={version}");
}
