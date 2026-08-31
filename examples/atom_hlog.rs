//! Construct the Hyperbolica Hlog head and integrate it as an Atom.

use hyperbolica::prelude::*;

fn main() -> AtomIntegrationResult<()> {
    let x = symbol!("example_hlog_x");
    let hlog = heads().hlog.call((x, Atom::zero(), -1));
    let integrand = hlog / (x + 1).pow(2);
    let options = AtomIntegrationOptions {
        check_divergences: true,
        ..AtomIntegrationOptions::default()
    };
    let result = integrate_atom(&integrand, &[x], &options)?;

    println!("{}", result.to_atom()?);
    Ok(())
}
