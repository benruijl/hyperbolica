use hyperbolica::core::{Poly, PolyCtx, Rat};

fn main() -> hyperbolica::Result<()> {
    let ctx = PolyCtx::new(["x", "y"])?;

    let polynomial = Poly::parse(ctx.clone(), "(x-y)*(x+y)")?;
    println!("expanded: {polynomial}");
    println!("factorization:");
    for (factor, exponent) in polynomial.factor().factors {
        println!("  ({factor})^{exponent}");
    }

    let rational = Rat::parse(ctx.clone(), "1/(x*(1+x)*(x-y))")?;
    println!("rational function: {rational}");

    let resultant =
        Poly::parse(ctx.clone(), "x^2+y*x+1")?.resultant(&Poly::parse(ctx, "x-y")?, 0)?;
    println!("resultant in x: {resultant}");
    Ok(())
}
