//! Search a fixed gauge, scan every Cheng--Wu gauge, and build the exact
//! factor-prediction table used by an integration chain.

use hyperbolica::core::{Poly, PolyCtx};
use hyperbolica::integrator::factor_table::{FactorTableLimits, factor_table};
use hyperbolica::integrator::lr_scan::{KeepRule, ScanExponent, ScanOptions, find_lr_orders_scan};
use hyperbolica::integrator::lr_search::{LrSearchOptions, find_lr_orders};

fn main() -> hyperbolica::Result<()> {
    // A gauge-fixed, two-variable system with a quadratic square-root
    // obligation.  Carry-discharge makes both integration orders visible.
    let gauged_ctx = PolyCtx::new(["x", "y", "s"])?;
    let gauged = vec![vec![
        Poly::parse(gauged_ctx.clone(), "x+y+1")?,
        Poly::parse(gauged_ctx.clone(), "x^2+y^2+x*y+s")?,
    ]];
    let best = find_lr_orders(
        &gauged,
        &[0, 1],
        LrSearchOptions {
            allow_algebraic_letters: true,
            carry_discharge: true,
            ..LrSearchOptions::default()
        },
    )?;
    println!(
        "best fixed-gauge order: {:?}; carried square roots: {}",
        best.order, best.carried_sqrts
    );

    let table = factor_table(&gauged, &best.order, true, FactorTableLimits::default())?;
    println!(
        "factor table: {} interned factors, {} pairs, {} singletons",
        table.intern_polys.len(),
        table.pairs.len(),
        table.singletons.len()
    );

    // Its homogeneous three-variable lift is projective:
    // (-1+2 eps)*deg(U) + (-1-eps)*deg(F) == -3.
    let projective_ctx = PolyCtx::new(["x", "y", "z", "s"])?;
    let projective = vec![vec![
        Poly::parse(projective_ctx.clone(), "x+y+z")?,
        Poly::parse(projective_ctx, "x^2+y^2+x*y+s*z^2")?,
    ]];
    let scan = find_lr_orders_scan(
        &projective,
        &[0, 1, 2],
        &[vec![
            ScanExponent { a: -1, b: 2 },
            ScanExponent { a: -1, b: -1 },
        ]],
        ScanOptions {
            keep_rule: KeepRule::FindRoots,
            ..ScanOptions::default()
        },
    )?;
    println!(
        "projective: {}; admissible gauge/order pairs: {}",
        scan.projective,
        scan.orders.len()
    );
    Ok(())
}
