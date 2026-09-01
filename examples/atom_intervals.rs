//! Integrate native Symbolica Atoms over finite and parameterized intervals.

use hyperbolica::prelude::*;

fn main() -> AtomIntegrationResult<()> {
    let (x, a) = symbol!("x", "a");
    let options = AtomIntegrationOptions {
        check_divergences: true,
        parallel: false,
        ..AtomIntegrationOptions::default()
    };

    let finite = integrate_atom_over(
        &Atom::one(),
        &[x],
        &[IntegrationInterval::finite(2, 5)],
        &options,
    )?;
    println!("integral of 1 from 2 to 5 = {}", finite.to_atom()?);

    let tail = IntegrationInterval::new(
        IntegrationEndpoint::finite(a),
        IntegrationEndpoint::PositiveInfinity,
    );
    let rational = Atom::one() / (x + 1).pow(2);
    let parameterized = integrate_atom_over(&rational, &[x], &[tail], &options)?;
    println!(
        "integral of 1/(1+x)^2 from a to +Infinity = {}",
        parameterized.to_atom()?
    );

    Ok(())
}
