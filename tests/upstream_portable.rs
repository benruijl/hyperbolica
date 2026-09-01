//! Portable mathematical regressions derived from HyperFLINT's public tests.
//!
//! This remains one serial test because restricted Symbolica builds bind an
//! instance to their first calling thread. The checked-in upstream inventory
//! in `docs/upstream-test-matrix.md` records the provenance of every case.

use std::collections::BTreeSet;
use std::sync::Arc;

use hyperbolica::algebra::{PartialFractionization, partial_fractions};
use hyperbolica::core::{
    Poly, PolyCtx, Rat, ZWTable, build_fn_index_maps, recombine_rat_split, split_rat_by_w_monomial,
};
use hyperbolica::integrator::factor_table::{
    FactorTable, FactorTableLimits, FactoredObject, factor_table,
};
use hyperbolica::integrator::lr_scan::{KeepRule, ScanExponent, ScanOptions, find_lr_orders_scan};
use hyperbolica::reduce::{load_mzv_expansion, load_mzv_expansion_with_options};
use hyperbolica::symbols::{log_two_atom, mzv_atom};

fn parse_poly(ctx: &Arc<PolyCtx>, expression: &str) -> Poly {
    Poly::parse(ctx.clone(), expression).unwrap()
}

fn factor_products(
    ctx: &Arc<PolyCtx>,
    table: &FactorTable,
    object: &FactoredObject,
) -> (Poly, Poly) {
    let mut numerator = Poly::one(ctx.clone());
    let mut denominator = Poly::one(ctx.clone());
    for &(id, exponent) in &object.factors {
        let power = table.intern_polys[id].pow(exponent.unsigned_abs() as usize);
        if exponent > 0 {
            numerator = numerator.try_mul(&power).unwrap();
        } else {
            denominator = denominator.try_mul(&power).unwrap();
        }
    }
    (numerator, denominator)
}

fn factor_constant(ctx: &Arc<PolyCtx>, object: &FactoredObject) -> Poly {
    Poly::from_rational(ctx.clone(), object.constant.clone())
}

fn assert_factor_table_contracts(ctx: &Arc<PolyCtx>, table: &FactorTable) {
    for pair in &table.pairs {
        let f = &table.intern_polys[pair.f_id];
        let g = &table.intern_polys[pair.g_id];
        let lc_f = f.coefficient_of(pair.var_idx, 1).unwrap();
        let lc_g = g.coefficient_of(pair.var_idx, 1).unwrap();
        let numerator = lc_g
            .try_mul(f)
            .unwrap()
            .try_sub(&lc_f.try_mul(g).unwrap())
            .unwrap();
        assert_eq!(numerator, -&f.resultant(g, pair.var_idx).unwrap());

        let (num_product, den_product) = factor_products(ctx, table, &pair.difference);
        let lhs = numerator.try_mul(&den_product).unwrap();
        let rhs = factor_constant(ctx, &pair.difference)
            .try_mul(&num_product)
            .unwrap()
            .try_mul(&lc_f)
            .unwrap()
            .try_mul(&lc_g)
            .unwrap();
        assert_eq!(lhs, rhs);
    }

    for singleton in &table.singletons {
        let letter = &table.intern_polys[singleton.id];
        for coefficient in &singleton.coefficients {
            let target = letter
                .coefficient_of(singleton.var_idx, coefficient.power)
                .unwrap();
            let (num_product, den_product) = factor_products(ctx, table, &coefficient.object);
            assert_eq!(
                target.try_mul(&den_product).unwrap(),
                factor_constant(ctx, &coefficient.object)
                    .try_mul(&num_product)
                    .unwrap()
            );
        }
        if let Some(discriminant) = &singleton.discriminant {
            let target = letter.discriminant(singleton.var_idx).unwrap();
            let (num_product, den_product) = factor_products(ctx, table, discriminant);
            assert_eq!(
                target.try_mul(&den_product).unwrap(),
                factor_constant(ctx, discriminant)
                    .try_mul(&num_product)
                    .unwrap()
            );
        }
    }
}

fn reconstruct_partial_fractions(decomposition: &PartialFractionization, variable: usize) -> Rat {
    let ctx = decomposition.polynomial_part.ctx().clone();
    let x = Rat::from_poly(Poly::generator(ctx, variable).unwrap());
    let mut output = decomposition.polynomial_part.clone();
    for pole in &decomposition.poles {
        assert_eq!(pole.coefs.len(), pole.multiplicity);
        let base = x.try_sub(&pole.pole).unwrap();
        for (order, coefficient) in pole.coefs.iter().enumerate() {
            output = output
                .try_add(
                    &coefficient
                        .try_div(&base.pow(i64::try_from(order + 1).unwrap()).unwrap())
                        .unwrap(),
                )
                .unwrap();
        }
    }
    output
}

fn scale(value: &Rat, numerator: i64, denominator: i64) -> Rat {
    let ctx = value.ctx().clone();
    value
        .try_mul(&Rat::from_int(ctx.clone(), numerator))
        .unwrap()
        .try_div(&Rat::from_int(ctx, denominator))
        .unwrap()
}

#[test]
fn portable_upstream_regressions_are_preserved() {
    // UQ5: exact projectivity, gauge-exhaustive strict rejection, and the
    // published FindRoots order/count oracle.
    let uq5_ctx = PolyCtx::new([
        "x1", "x2", "x3", "x4", "x5", "qq1", "qq2", "wb1", "wb2", "yb",
    ])
    .unwrap();
    let uq5_groups = vec![vec![
        parse_poly(&uq5_ctx, "x1+x2+x3"),
        parse_poly(
            &uq5_ctx,
            "-qq1*x1*x2-qq2*x1*x3+2*wb1*x3*x4-x4^2+2*wb2*x2*x5-x5^2+2*yb*x4*x5",
        ),
    ]];
    let uq5_xvars = [0, 1, 2, 3, 4];
    let uq5_exponents = vec![vec![
        ScanExponent { a: 1, b: 2 },
        ScanExponent { a: -3, b: -1 },
    ]];
    let strict = find_lr_orders_scan(
        &uq5_groups,
        &uq5_xvars,
        &uq5_exponents,
        ScanOptions {
            keep_rule: KeepRule::Strict,
            max_orders: 121,
            ..ScanOptions::default()
        },
    )
    .unwrap();
    assert!(strict.projective);
    assert!(!strict.truncated);
    assert!(strict.orders.is_empty());

    let roots = find_lr_orders_scan(
        &uq5_groups,
        &uq5_xvars,
        &uq5_exponents,
        ScanOptions {
            keep_rule: KeepRule::FindRoots,
            max_orders: 121,
            ..ScanOptions::default()
        },
    )
    .unwrap();
    assert!(roots.projective);
    assert!(!roots.truncated);
    assert_eq!(roots.orders.len(), 120);
    assert_eq!(
        roots
            .orders
            .iter()
            .map(|order| order.gauge)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([0, 1, 2, 3, 4])
    );
    let collaborator = roots
        .orders
        .iter()
        .find(|order| order.gauge == 4 && order.order == [1, 2, 0, 3])
        .expect("UQ5 collaborator order at gauge x5");
    assert_eq!(collaborator.carried_sqrts, 2);

    // Every emitted factor-table object is exact. Include the two upstream
    // edge cases that were not previously represented in the Rust suite.
    let factor_ctx = PolyCtx::new(["x", "y"]).unwrap();
    // Upstream's original linear fixture predates the Brown constant-term
    // reduction: its trailing coefficient is now part of the stage pool and
    // therefore cannot exercise the advertised fallback. A quadratic middle
    // coefficient is not one of the leading/constant/discriminant candidates,
    // so this preserves the intended out-of-pool singleton contract.
    let oop = factor_table(
        &[vec![parse_poly(&factor_ctx, "x^2+(y^2+y+1)*x+1")]],
        &[0],
        true,
        FactorTableLimits::default(),
    )
    .unwrap();
    assert!(oop.stats.oop >= 1);
    assert!(oop.singletons.iter().any(|entry| {
        entry.var_idx == 0
            && entry
                .coefficients
                .iter()
                .any(|coefficient| coefficient.power == 1 && coefficient.object.oop)
    }));
    assert_factor_table_contracts(&factor_ctx, &oop);

    let zero_difference = factor_table(
        &[vec![
            parse_poly(&factor_ctx, "x*y+y^2"),
            parse_poly(&factor_ctx, "x+y"),
        ]],
        &[0, 1],
        false,
        FactorTableLimits::default(),
    )
    .unwrap();
    assert_eq!(zero_difference.stages[0].pair_count, 1);
    assert_eq!(
        zero_difference.pairs[0].difference.constant.to_string(),
        "0"
    );
    assert!(zero_difference.pairs[0].difference.factors.is_empty());
    assert_factor_table_contracts(&factor_ctx, &zero_difference);

    // Two non-unit, parameter-dependent pole systems pin multiplicity and
    // exact reconstruction without relying on printer order.
    let pf_ctx = PolyCtx::new(["x", "y", "z"]).unwrap();
    for source in [
        "(x^2+y*z+1)/((z*x-y)^2*(y*x-z))",
        "(x^2+y+z)/(((y+1)*x-y)^2*((z+2)*x-1))",
    ] {
        let value = Rat::parse(pf_ctx.clone(), source).unwrap();
        let decomposition = partial_fractions(&value, 0).unwrap();
        assert_eq!(decomposition.poles.len(), 2, "source: {source}");
        assert_eq!(
            decomposition
                .poles
                .iter()
                .map(|pole| pole.multiplicity)
                .sum::<usize>(),
            3,
            "source: {source}"
        );
        assert_eq!(reconstruct_partial_fractions(&decomposition, 0), value);
    }

    // Native RationalPolynomial normalization makes construction paths with
    // shared integer content structurally identical, including the 12-var
    // regime used by the upstream Smirnov audit.
    let content_ctx = PolyCtx::new((0..12).map(|index| format!("x{index}"))).unwrap();
    let a_raw = Rat::parse(content_ctx.clone(), "(2*x0+4*x1)/(2*x2+2)").unwrap();
    let a_reduced = Rat::parse(content_ctx.clone(), "(x0+2*x1)/(x2+1)").unwrap();
    let b_raw = Rat::parse(content_ctx.clone(), "(3*x3-3*x4)/(3*x5+3*x6+3)").unwrap();
    let b_reduced = Rat::parse(content_ctx.clone(), "(x3-x4)/(x5+x6+1)").unwrap();
    assert_eq!(a_raw, a_reduced);
    assert_eq!(b_raw, b_reduced);
    assert_eq!(
        a_raw.try_add(&b_raw).unwrap(),
        a_reduced.try_add(&b_reduced).unwrap()
    );

    // Broaden the wide/narrow split corpus with W-only and mixed
    // denominators plus higher W-side exponents.
    let wide = PolyCtx::new(["x", "y", "s", "t"]).unwrap();
    let narrow = PolyCtx::new(["x", "y"]).unwrap();
    let maps = build_fn_index_maps(&wide, &narrow).unwrap();
    for (numerator, denominator) in [
        ("x+y", "1"),
        ("s", "1"),
        ("x*y", "s-1"),
        ("x", "x+s"),
        ("s^2*t*x+s*t^2*y", "1"),
        ("s+t+s*t+1", "x+s"),
    ] {
        let value = Rat::new(parse_poly(&wide, numerator), parse_poly(&wide, denominator)).unwrap();
        let mut table = ZWTable::new(wide.clone());
        let split = split_rat_by_w_monomial(&value, narrow.clone(), &mut table, &maps).unwrap();
        assert_eq!(recombine_rat_split(&split, &table, &maps).unwrap(), value);
    }

    // Production MZV-table shape and classical low-weight identities.
    let mzv_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/mzv_reductions.json");
    let expansion = load_mzv_expansion(mzv_path).unwrap();
    assert_eq!(expansion.basis_names.len(), 10);
    assert_eq!(expansion.basis_names[0], "Log2");
    assert_eq!(expansion.expansion.len(), 700);
    assert_eq!(
        expansion
            .expansion
            .iter()
            .filter(|(name, value)| {
                name.strip_prefix("mzv_")
                    .is_some_and(|indices| indices.split('_').all(|index| index == "1"))
                    && value.is_zero()
            })
            .count(),
        8
    );
    let z2 = Rat::from_atom(expansion.basis_ctx.clone(), mzv_atom(&[2]).as_view()).unwrap();
    let z3 = Rat::from_atom(expansion.basis_ctx.clone(), mzv_atom(&[3]).as_view()).unwrap();
    let z5 = Rat::from_atom(expansion.basis_ctx.clone(), mzv_atom(&[5]).as_view()).unwrap();
    assert_eq!(
        expansion.expansion["mzv_4"],
        scale(&z2.pow(2).unwrap(), 2, 5)
    );
    assert_eq!(
        expansion.expansion["mzv_6"],
        scale(&z2.pow(3).unwrap(), 8, 35)
    );
    assert_eq!(
        expansion.expansion["mzv_8"],
        scale(&z2.pow(4).unwrap(), 24, 175)
    );
    assert_eq!(
        expansion.expansion["mzv_2_2"],
        scale(&z2.pow(2).unwrap(), 3, 10)
    );
    assert_eq!(
        expansion.expansion["mzv_2_3"],
        z2.try_mul(&z3)
            .unwrap()
            .try_mul(&Rat::from_int(expansion.basis_ctx.clone(), 3))
            .unwrap()
            .try_sub(&scale(&z5, 11, 2))
            .unwrap()
    );
    assert_eq!(
        expansion.expansion["mzv_3_2"],
        z2.try_mul(&z3)
            .unwrap()
            .try_mul(&Rat::from_int(expansion.basis_ctx.clone(), -2))
            .unwrap()
            .try_add(&scale(&z5, 9, 2))
            .unwrap()
    );

    // Ingest both public chained-rule files. The default loader must reject
    // them as non-production-flat, while the explicit validation mode fully
    // substitutes prior rules without leaving wide-context LHS symbols.
    let test_data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let benign_path = test_data.join("mzv_reductions_chained_test.json");
    assert!(load_mzv_expansion(&benign_path).is_err());
    let benign = load_mzv_expansion_with_options(&benign_path, true).unwrap();
    let benign_z2 = Rat::from_atom(benign.basis_ctx.clone(), mzv_atom(&[2]).as_view()).unwrap();
    assert_eq!(
        benign.expansion["mzv_6"],
        benign_z2
            .pow(3)
            .unwrap()
            .try_mul(&Rat::from_int(benign.basis_ctx.clone(), 5))
            .unwrap()
    );

    let adversarial_path = test_data.join("mzv_reductions_chained_adversarial.json");
    assert!(load_mzv_expansion(&adversarial_path).is_err());
    let adversarial = load_mzv_expansion_with_options(&adversarial_path, true).unwrap();
    let adversarial_z2 =
        Rat::from_atom(adversarial.basis_ctx.clone(), mzv_atom(&[2]).as_view()).unwrap();
    let log_two = Rat::from_atom(adversarial.basis_ctx.clone(), log_two_atom().as_view()).unwrap();
    let expanded_mzv4 = adversarial_z2
        .try_mul(&log_two.pow(2).unwrap())
        .unwrap()
        .try_sub(&adversarial_z2.pow(2).unwrap())
        .unwrap();
    assert_eq!(
        adversarial.expansion["mzv_8"],
        expanded_mzv4.pow(2).unwrap().negated()
    );
}
