use super::*;

#[test]
fn pure_rational_subexpressions_are_folded() {
    let parsed = parse_expression("1/(1+x)+x/(1+x)", &[], false).unwrap();
    assert_eq!(parsed.expr.to_string(), "1");
    assert_eq!(parsed.augmented_vars, ["x"]);
}

#[test]
fn log_rewrites_to_weight_one_hlog() {
    let parsed = parse_expression("Log[x]", &[], false).unwrap();
    assert_eq!(parsed.expr.to_string(), "Hlog[x,[0]]");
}

#[test]
fn power_binds_tighter_than_unary_minus() {
    let parsed = parse_expression("-Hlog[x,{0}]^2", &[], false).unwrap();
    assert_eq!(parsed.expr.to_string(), "Times[-1,Power[Hlog[x,[0]],2]]");
}

#[test]
fn powers_are_right_associative() {
    let parsed = parse_expression("x^2^3", &[], false).unwrap();
    assert_eq!(parsed.expr.to_string(), "x^8");
}

#[test]
fn lazy_top_sum_preserves_only_the_outer_sum() {
    let parsed = parse_expression("1/(1+x)+x", &[], true).unwrap();
    assert!(matches!(parsed.expr, Expr::Plus(_)));
    assert_eq!(parsed.expr.to_string(), "Plus[1/(x + 1),x]");
}

#[test]
fn mathematica_integer_subscripts_remain_inert_variables() {
    let parsed = parse_expression("m[1,2]+1", &[], false).unwrap();
    assert_eq!(parsed.augmented_vars, ["m[1,2]"]);
    let variable = Rat::from_poly(Poly::generator(parsed.ctx.clone(), 0).unwrap());
    let expected = variable.try_add(&Rat::one(parsed.ctx.clone())).unwrap();
    assert_eq!(parsed.expr, Expr::leaf(expected));
}

#[test]
fn registered_aliases_share_one_context_slot_and_cancel() {
    for (legacy_name, indexed_name) in [
        ("mzv_3", "MZV[3]"),
        ("mzv_2_3", "MZV[2, 3]"),
        ("Wm_1", "Wm[1]"),
    ] {
        let source = format!("{indexed_name}-{legacy_name}");
        let parsed = parse_expression(&source, &[legacy_name.into()], false).unwrap();
        assert_eq!(parsed.augmented_vars, [legacy_name]);
        assert_eq!(parsed.ctx.len(), 1);
        assert_eq!(parsed.expr, Expr::leaf(Rat::zero(parsed.ctx)));

        let parsed = parse_expression(&source, &[indexed_name.into()], false).unwrap();
        assert_eq!(parsed.augmented_vars, [indexed_name]);
        assert_eq!(parsed.ctx.len(), 1);
        assert_eq!(parsed.expr, Expr::leaf(Rat::zero(parsed.ctx)));
    }
}

#[test]
fn indexed_mzv_coefficient_parses_with_the_reserved_standard_basis() {
    let table = crate::reduce::standard_mzv_reductions();
    let variables =
        crate::reduce::build_narrow_var_list(&table, &["x".into()], "MZV[3]*Log[1+x]/(x*(1+x))");
    let indexed = parse_expression("MZV[3]*Log[1+x]/(x*(1+x))", &variables, false).unwrap();
    let legacy = parse_expression("mzv_3*Log[1+x]/(x*(1+x))", &variables, false).unwrap();
    assert_eq!(indexed.augmented_vars, variables);
    assert_eq!(indexed.expr, legacy.expr);
}

#[test]
fn structural_deduplication_preserves_namespaces_and_other_heads() {
    let names = [
        "parse_left::x".into(),
        "parse_right::x".into(),
        "MZV[3]".into(),
    ];
    let parsed = parse_expression("x+m[3]+MZV[3]", &names, false).unwrap();
    assert_eq!(parsed.ctx.len(), 5);
    assert_eq!(
        parsed.augmented_vars,
        ["parse_left::x", "parse_right::x", "MZV[3]", "x", "m[3]"]
    );
    assert_ne!(
        parsed.ctx.variable_atom(0).unwrap(),
        parsed.ctx.variable_atom(1).unwrap()
    );
    assert_ne!(
        parsed.ctx.variable_atom(1).unwrap(),
        parsed.ctx.variable_atom(3).unwrap()
    );
    assert_ne!(
        parsed.ctx.variable_atom(2).unwrap(),
        parsed.ctx.variable_atom(4).unwrap()
    );
    assert!(parse_expression("x", &["x".into(), "x".into()], false).is_err());
}
