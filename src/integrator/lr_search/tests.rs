use std::collections::HashSet;
use std::sync::Arc;

use crate::core::{Poly, PolyCtx};

use super::*;

fn parse(ctx: &Arc<PolyCtx>, expression: &str) -> Poly {
    Poly::parse(ctx.clone(), expression).unwrap()
}

#[test]
fn fubini_step_includes_endpoint_and_pair_resultant_factors() {
    let ctx = PolyCtx::new(["x", "y"]).unwrap();
    let input = vec![parse(&ctx, "1+x*y"), parse(&ctx, "x+y")];
    let output = st_fubini_lr(&input, 0).unwrap();
    let keys = output.into_iter().collect::<HashSet<_>>();
    assert!(keys.contains(&parse(&ctx, "y").canonical_proportional_form()));
    assert!(keys.contains(&parse(&ctx, "y-1").canonical_proportional_form()));
    assert!(keys.contains(&parse(&ctx, "y+1").canonical_proportional_form()));
}

#[test]
fn proportional_dedup_is_structural_and_preserves_first_encounter_order() {
    let ctx = PolyCtx::new(["x", "y"]).unwrap();
    let input = vec![
        parse(&ctx, "2*x+2*y"),
        parse(&ctx, "-x-y"),
        parse(&ctx, "x-y"),
        parse(&ctx, "3*x-3*y"),
        parse(&ctx, "7"),
    ];
    let expected = vec![
        parse(&ctx, "x+y").canonical_proportional_form(),
        parse(&ctx, "x-y").canonical_proportional_form(),
    ];

    assert_eq!(dedup_proportional(&input), expected);
}

#[test]
fn structural_reduction_caches_do_not_change_deterministic_ordering() {
    let ctx = PolyCtx::new(["x", "y", "z"]).unwrap();
    let input = vec![
        parse(&ctx, "1+x*y"),
        parse(&ctx, "x+y*z"),
        parse(&ctx, "x+z"),
    ];
    let mut engine = ReductionEngine::new().unwrap();
    let cold = engine.reduce(&input, 0, None).unwrap();
    let warm = engine.reduce(&input, 0, None).unwrap();
    let independent = st_fubini_lr(&input, 0).unwrap();

    assert_eq!(warm, cold);
    assert_eq!(independent, cold);
}

#[test]
fn order_search_prefers_the_lower_leaf_count_path() {
    let ctx = PolyCtx::new(["x", "y"]).unwrap();
    let groups = vec![vec![parse(&ctx, "1+x*y"), parse(&ctx, "x+y")]];
    let result = find_lr_orders(&groups, &[0, 1], LrSearchOptions::default()).unwrap();
    assert!(!result.is_nolr());
    assert_eq!(result.order.len(), 2);
    assert_eq!(
        result.order.iter().copied().collect::<HashSet<_>>(),
        HashSet::from([0, 1])
    );
}

#[test]
fn degree_and_future_variable_guard_matches_hyperflint() {
    let ctx = PolyCtx::new(["x", "z", "y"]).unwrap();
    let linear = parse(&ctx, "x+y");
    let quadratic = parse(&ctx, "y*x^2+x+1");
    let future = parse(&ctx, "z*x^2+x+1");
    let cubic = parse(&ctx, "x^3+y");
    assert!(lr_letter_admissible(&linear, 0, &[1], 1).unwrap());
    assert!(!lr_letter_admissible(&quadratic, 0, &[1], 1).unwrap());
    assert!(lr_letter_admissible(&quadratic, 0, &[1], 2).unwrap());
    assert!(!lr_letter_admissible(&future, 0, &[1], 2).unwrap());
    assert!(!lr_letter_admissible(&cubic, 0, &[1], 2).unwrap());
}

#[test]
fn euler_filter_on_off_parity_for_massless_box() {
    let ctx = PolyCtx::new(["x1", "x2", "x3", "x4", "s", "t"]).unwrap();
    let group = ["x1+x2+x3+x4+s*x1*x3+t*x2*x4", "x1", "x2", "x3", "x4"]
        .map(|expression| parse(&ctx, expression))
        .to_vec();
    let off = find_lr_orders(
        std::slice::from_ref(&group),
        &[0, 1, 2, 3],
        LrSearchOptions::default(),
    )
    .unwrap();
    let on = find_lr_orders(
        &[group],
        &[0, 1, 2, 3],
        LrSearchOptions {
            euler_filter: true,
            ..LrSearchOptions::default()
        },
    )
    .unwrap();
    assert_eq!(off.order, on.order);
    assert_eq!(off.score, on.score);
    assert!(!off.is_nolr() && !on.is_nolr());
}
