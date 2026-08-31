use std::sync::Arc;

use crate::core::{Poly, PolyCtx};

use super::*;

fn parse(ctx: &Arc<PolyCtx>, expression: &str) -> Poly {
    Poly::parse(ctx.clone(), expression).unwrap()
}

#[test]
fn projectivity_and_inhomogeneity_match_the_doppio_oracle() {
    let ctx = PolyCtx::new([
        "x1", "x2", "x3", "x4", "x5", "qq1", "qq2", "wb1", "wb2", "yb",
    ])
    .unwrap();
    let group = vec![
        parse(&ctx, "x1+x2+x3"),
        parse(
            &ctx,
            "-qq1*x1*x2-qq2*x1*x3+2*wb1*x3*x4-x4^2+2*wb2*x2*x5-x5^2+2*yb*x4*x5",
        ),
    ];
    assert!(
        projective_input(
            std::slice::from_ref(&group),
            &[0, 1, 2, 3, 4],
            &[vec![
                ScanExponent { a: 1, b: 2 },
                ScanExponent { a: -3, b: -1 }
            ]],
        )
        .unwrap()
    );
    assert!(
        !projective_input(
            &[group],
            &[0, 1, 2, 3, 4],
            &[vec![
                ScanExponent { a: 1, b: 0 },
                ScanExponent { a: -2, b: 0 }
            ]],
        )
        .unwrap()
    );
}

#[test]
fn conic_square_semantics_reject_irrational_and_negative_units() {
    let ctx = PolyCtx::new(["x", "z", "s", "q"]).unwrap();
    assert!(conic_rationalizable(&parse(&ctx, "z^2*x^2+s*x+1"), 0).unwrap());
    assert!(conic_rationalizable(&parse(&ctx, "4*x^2+s*x+9"), 0).unwrap());
    assert!(!conic_rationalizable(&parse(&ctx, "2*x^2+s*x+1"), 0).unwrap());
    assert!(!conic_rationalizable(&parse(&ctx, "-x^2+s*x+1"), 0).unwrap());
    assert!(!conic_rationalizable(&parse(&ctx, "q*x^2+s*x+1"), 0).unwrap());
}

#[test]
fn find_roots_judgment_handles_zero_mixed_and_terminal_discriminants() {
    let ctx = PolyCtx::new(["x", "z", "s", "t", "y", "g"]).unwrap();
    let zero_disc = fr_judge(&parse(&ctx, "g*x^2-2*g*x*z+g*z^2"), 0, &[1], &[0, 1]).unwrap();
    assert!(zero_disc.ok && zero_disc.carry.is_empty() && zero_disc.kin == 0);

    let mixed = fr_judge(&parse(&ctx, "x^2-s*z^2+t*z^2+s*y-t*y"), 0, &[1], &[0, 1]).unwrap();
    assert!(mixed.ok && mixed.carry.len() == 1 && mixed.kin == 1);

    let terminal = fr_judge(&parse(&ctx, "s*z^2+t*z+1"), 1, &[], &[0, 1]).unwrap();
    assert!(terminal.ok && terminal.terminal == 1);
    assert!(
        !fr_judge(&parse(&ctx, "x^3-s"), 0, &[1], &[0, 1])
            .unwrap()
            .ok
    );
}

#[test]
fn carry_ledgers_deduplicate_structurally_across_letters_and_steps() {
    let ctx = PolyCtx::new(["x", "z", "s"]).unwrap();
    let letters = vec![parse(&ctx, "x^2+x-s*z"), parse(&ctx, "x^2-x-s*z")];
    let mut state = PathState::default();

    assert!(step_fr_judge(&letters, 0, None, &[1], &[0, 1], &mut state).unwrap());
    assert_eq!(state.nsq, 1);
    assert_eq!(state.carried.len(), 1);
    assert_eq!(state.minted.len(), 1);

    assert!(step_fr_judge(&letters, 0, None, &[1], &[0, 1], &mut state).unwrap());
    assert_eq!(state.nsq, 1);
    assert_eq!(state.carried.len(), 1);
    assert_eq!(state.minted.len(), 1);
}

#[test]
fn projective_scan_accepts_native_euler_filter() {
    let ctx = PolyCtx::new(["x", "y"]).unwrap();
    let group = vec![parse(&ctx, "x+y")];
    let result = find_lr_orders_scan(
        &[group],
        &[0, 1],
        &[vec![ScanExponent { a: -2, b: 0 }]],
        ScanOptions {
            euler_filter: true,
            ..ScanOptions::default()
        },
    )
    .unwrap();
    assert!(result.projective);
    assert!(!result.orders.is_empty());
}
