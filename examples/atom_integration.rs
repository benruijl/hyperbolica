//! Integrate a Symbolica Atom directly, without a parsing or JSON boundary.

use hyperbolica::prelude::*;

fn main() -> AtomIntegrationResult<()> {
    let (x, y) = symbol!("example_atom_x", "example_atom_y");
    let integrand = Atom::one() / ((x + 1).pow(2) * (y + 1).pow(2));
    let options = AtomIntegrationOptions {
        check_divergences: true,
        parallel: false,
        ..AtomIntegrationOptions::default()
    };

    let prepared = prepare_atom(&integrand, &[x, y])?;
    let result = integrate_prepared_atom(&prepared, &options)?;

    assert_eq!(result.integration_variables(), &[x, y]);
    assert_eq!(result.to_atom()?, Atom::one());
    println!("{}", result.to_atom()?);
    Ok(())
}
