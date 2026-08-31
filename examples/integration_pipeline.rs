use hyperbolica::core::{PolyCtx, Rat};
use hyperbolica::integrator::{ShuffleEntry, hyperflint};
use hyperbolica::reduce::MzvReductionTable;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = PolyCtx::new(["x", "y"])?;
    let integrand = Rat::parse(ctx.clone(), "1/((1+x)^2*(1+y)^2)")?;
    let input = vec![ShuffleEntry::new(integrand, Vec::new())];

    let result = hyperflint(&ctx, &input, &[0, 1], &MzvReductionTable::default())?;
    println!("{result:#?}");
    Ok(())
}
