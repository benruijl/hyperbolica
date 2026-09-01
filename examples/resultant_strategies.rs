//! Time Symbolica's public resultant strategies on the locked dense fixture.

use std::time::Instant;

use hyperbolica::core::{Poly, PolyCtx, ResultantStrategy};

fn parse_strategy(value: &str) -> ResultantStrategy {
    match value {
        "auto" => ResultantStrategy::Auto,
        "ducos" => ResultantStrategy::Ducos,
        "rational-ducos" => ResultantStrategy::RationalDucos,
        "brown" => ResultantStrategy::Brown,
        "primitive" => ResultantStrategy::Primitive,
        "crt" => ResultantStrategy::Crt,
        _ => panic!("strategy must be one of: auto, ducos, rational-ducos, brown, primitive, crt"),
    }
}

fn inputs(case: &str) -> (&'static [&'static str], &'static str, &'static str) {
    match case {
        "locked" => (
            &["x", "y", "z", "s", "t"],
            "x^7+(y+z)*x^6+(y^2+s*z+1)*x^5+(y*z+s*t+2)*x^4+\
             (z^2+y*t+3)*x^3+(s*y-t*z+4)*x^2+(y^3+z^3+5)*x+s^2+t^2+1",
            "x^6+(y-z)*x^5+(y*z+s+2)*x^4+(y^2-z*t+3)*x^3+\
             (z^2+s*t+5)*x^2+(y*s-z*t+7)*x+y^2*z+s*t+11",
        ),
        "dense" => (
            &["x", "y", "z"],
            "1+(2+y^2+z^3)*x+(3+y^3+z^2)*x^2+(4+y+z)*x^3+\
             (5+y^2+z^3)*x^4+(6+y^3+z^2)*x^5+(7+y+z)*x^6+(8+y^2+z^3)*x^7",
            "1+(3+y^3-z^2)*x+(5+y^2-z^3)*x^2+(7+y-z)*x^3+\
             (9+y^3-z^2)*x^4+(11+y^2-z^3)*x^5+(13+y-z)*x^6",
        ),
        "lacunary" => (
            &["x", "y", "z"],
            "(y+1)*x^18+(z+2)*x^13+(y*z+3)*x^7+(y^2-z)*x^2+1",
            "(z+1)*x^11+(y-2)*x^8+(y+z)*x^3+2",
        ),
        _ => panic!("case must be one of: locked, dense, lacunary"),
    }
}

fn main() -> hyperbolica::Result<()> {
    let strategy = std::env::args()
        .nth(1)
        .as_deref()
        .map(parse_strategy)
        .unwrap_or_default();
    let case = std::env::args().nth(2).unwrap_or_else(|| "locked".into());
    let (variables, left, right) = inputs(&case);
    let ctx = PolyCtx::new(variables.iter().copied())?;
    let left = Poly::parse(ctx.clone(), left)?;
    let right = Poly::parse(ctx, right)?;

    let started = Instant::now();
    let resultant = left.resultant_with_strategy(&right, 0, strategy)?;
    let elapsed = started.elapsed();
    println!(
        "{case}/{strategy:?}: {:.6} s, {} terms",
        elapsed.as_secs_f64(),
        resultant.n_terms()
    );
    Ok(())
}
