use super::*;

#[test]
fn factor_wire_array_matches_flint_structural_order() {
    let linear =
        evaluate_supported(&json!({"expr": "(x+2)*(x+10)", "vars": ["x"]}), "factor").unwrap();
    assert_eq!(linear["constant"], "1");
    assert_eq!(linear["factors"], json!([["10+x", 1], ["2+x", 1]]));

    let ctx = PolyCtx::new(["x"]).unwrap();
    let two = wire_poly(&Poly::parse(ctx.clone(), "x-2").unwrap());
    let ten = wire_poly(&Poly::parse(ctx, "x-10").unwrap());
    let positive =
        evaluate_supported(&json!({"expr": "(x-2)*(x-10)", "vars": ["x"]}), "factor").unwrap();
    assert_eq!(positive["factors"], json!([[two, 1], [ten, 1]]));

    let nonlinear = evaluate_supported(
        &json!({"expr": "(x^2+2)*(x^2+10)", "vars": ["x"]}),
        "factor",
    )
    .unwrap();
    assert_eq!(nonlinear["factors"], json!([["2+x^2", 1], ["10+x^2", 1]]));
}

#[test]
fn factor_wire_array_orders_multiplicity_before_native_base() {
    let output = evaluate_supported(
        &json!({
            "expr": "(x+2)^10*(x+10)^2",
            "vars": ["x"]
        }),
        "factor",
    )
    .unwrap();
    assert_eq!(output["factors"], json!([["10+x", 2], ["2+x", 10]]));

    let swapped = evaluate_supported(
        &json!({
            "expr": "(x+2)^2*(x+10)^10",
            "vars": ["x"]
        }),
        "factor",
    )
    .unwrap();
    assert_eq!(swapped["factors"], json!([["2+x", 2], ["10+x", 10]]));

    let degree = evaluate_supported(
        &json!({
            "expr": "(x+2)^10*(x^2+10)^2",
            "vars": ["x"]
        }),
        "factor",
    )
    .unwrap();
    assert_eq!(degree["factors"], json!([["10+x^2", 2], ["2+x", 10]]));
}

#[test]
fn factor_wire_array_uses_native_order_across_mixed_shapes() {
    let ctx = PolyCtx::new(["x", "y"]).unwrap();
    let rendered = |expression| wire_poly(&Poly::parse(ctx.clone(), expression).unwrap());

    let cubic_quadratic = evaluate_supported(
        &json!({
            "expr": "(x^2+y^2+1)*(x^3+y+1)",
            "vars": ["x", "y"]
        }),
        "factor",
    )
    .unwrap();
    assert_eq!(
        cubic_quadratic["factors"],
        json!([[rendered("x^3+y+1"), 1], [rendered("x^2+y^2+1"), 1]])
    );

    let multivariate_linear = evaluate_supported(
        &json!({
            "expr": "(x+y+2)*(x^2+y+1)",
            "vars": ["x", "y"]
        }),
        "factor",
    )
    .unwrap();
    assert_eq!(
        multivariate_linear["factors"],
        json!([[rendered("x^2+y+1"), 1], [rendered("x+y+2"), 1]])
    );

    let univariate_linear = evaluate_supported(
        &json!({
            "expr": "(x+2)*(x^2+y+1)",
            "vars": ["x", "y"]
        }),
        "factor",
    )
    .unwrap();
    assert_eq!(
        univariate_linear["factors"],
        json!([[rendered("x+2"), 1], [rendered("x^2+y+1"), 1]])
    );
}

#[test]
fn factor_wire_comparator_is_total_across_linear_and_nonlinear_shapes() {
    let ctx = PolyCtx::new(["x", "y"]).unwrap();
    let mut entries = ["x+2", "x+10", "x+y+2", "x^2+y+1", "x^2+y^2+1", "x^3+y+1"]
        .into_iter()
        .map(|expression| {
            let factor = Poly::parse(ctx.clone(), expression).unwrap();
            FactorWireEntry {
                wire: wire_poly(&factor),
                first_linear_pole: first_active_linear_pole(&factor),
                factor,
                exponent: 1,
            }
        })
        .collect::<Vec<_>>();

    for left in &entries {
        for right in &entries {
            assert_eq!(
                factor_wire_cmp(left, right),
                factor_wire_cmp(right, left).reverse()
            );
        }
    }
    for left in &entries {
        for middle in &entries {
            for right in &entries {
                if !factor_wire_cmp(left, middle).is_gt() && !factor_wire_cmp(middle, right).is_gt()
                {
                    assert!(!factor_wire_cmp(left, right).is_gt());
                }
            }
        }
    }

    entries.sort_unstable_by(factor_wire_cmp);
}

#[test]
fn linear_factor_wire_order_matches_numeric_and_symbolic_flint_oracles() {
    let negative = evaluate_supported(
        &json!({
            "poly": "(x+2)*(x+10)",
            "var": "x",
            "vars": ["x"]
        }),
        "linear_factors",
    )
    .unwrap();
    assert_eq!(negative["linear"], json!([[1, "-10", "1"], [1, "-2", "1"]]));

    let positive = evaluate_supported(
        &json!({
            "poly": "(x-2)*(x-10)",
            "var": "x",
            "vars": ["x"]
        }),
        "linear_factors",
    )
    .unwrap();
    assert_eq!(positive["linear"], json!([[1, "2", "1"], [1, "10", "1"]]));

    let symbolic = evaluate_supported(
        &json!({
            "poly": "(a*x+b)*(b*x+a)",
            "var": "x",
            "vars": ["x", "a", "b"]
        }),
        "linear_factors",
    )
    .unwrap();
    assert_eq!(symbolic["linear"], json!([[1, "-b", "a"], [1, "-a", "b"]]));

    let scaled = evaluate_supported(
        &json!({
            "poly": "(2*x+1)*(3*x+1)",
            "var": "x",
            "vars": ["x"]
        }),
        "linear_factors",
    )
    .unwrap();
    assert_eq!(scaled["linear"], json!([[1, "-1", "2"], [1, "-1", "3"]]));
}

#[test]
fn linear_factor_wire_order_uses_multiplicity_before_pole() {
    let first = evaluate_supported(
        &json!({
            "poly": "(x+2)^2*(x+10)^10",
            "var": "x",
            "vars": ["x"]
        }),
        "linear_factors",
    )
    .unwrap();
    assert_eq!(first["linear"], json!([[2, "-2", "1"], [10, "-10", "1"]]));

    let swapped = evaluate_supported(
        &json!({
            "poly": "(x+2)^10*(x+10)^2",
            "var": "x",
            "vars": ["x"]
        }),
        "linear_factors",
    )
    .unwrap();
    assert_eq!(swapped["linear"], json!([[2, "-10", "1"], [10, "-2", "1"]]));
}

#[test]
fn partial_fraction_wire_order_matches_exact_flint_oracles() {
    let evaluate = |expression| {
        evaluate_supported(
            &json!({
                "f": expression,
                "var": "x",
                "vars": ["x"]
            }),
            "partial_fractions",
        )
        .unwrap()
    };

    let negative = evaluate("1/((x+2)*(x+10))");
    assert_eq!(
        negative["poles"],
        json!([
            {"pole": "-10", "multiplicity": 1, "coefs": ["-1/8"]},
            {"pole": "-2", "multiplicity": 1, "coefs": ["1/8"]},
        ])
    );

    let positive = evaluate("1/((x-2)*(x-10))");
    assert_eq!(
        positive["poles"],
        json!([
            {"pole": "2", "multiplicity": 1, "coefs": ["-1/8"]},
            {"pole": "10", "multiplicity": 1, "coefs": ["1/8"]},
        ])
    );

    let scaled = evaluate("1/((2*x+1)*(3*x+1))");
    assert_eq!(
        scaled["poles"],
        json!([
            {"pole": "-1/2", "multiplicity": 1, "coefs": ["-1"]},
            {"pole": "-1/3", "multiplicity": 1, "coefs": ["1"]},
        ])
    );

    let multiplicities = evaluate("1/((x+2)^2*(x+10)^10)");
    assert_eq!(
        multiplicities["poles"],
        json!([
            {
                "pole": "-2",
                "multiplicity": 2,
                "coefs": ["-5/4294967296", "1/1073741824"],
            },
            {
                "pole": "-10",
                "multiplicity": 10,
                "coefs": [
                    "5/4294967296",
                    "9/1073741824",
                    "1/16777216",
                    "7/16777216",
                    "3/1048576",
                    "5/262144",
                    "1/8192",
                    "3/4096",
                    "1/256",
                    "1/64",
                ],
            },
        ])
    );
}

#[test]
fn nonlinear_factor_wire_order_uses_multiplicity_then_native_polynomial() {
    let plus = evaluate_supported(
        &json!({
            "poly": "(x^2+2)*(x^2+10)",
            "var": "x",
            "vars": ["x"]
        }),
        "linear_factors",
    )
    .unwrap();
    assert_eq!(
        plus["nonlinear"],
        json!([[1, "2+x^2", 2], [1, "10+x^2", 2]])
    );

    let minus = evaluate_supported(
        &json!({
            "poly": "(x^2-2)*(x^2-10)",
            "var": "x",
            "vars": ["x"]
        }),
        "linear_factors",
    )
    .unwrap();
    assert_eq!(
        minus["nonlinear"],
        json!([[1, "-10+x^2", 2], [1, "-2+x^2", 2]])
    );

    let multiplicity = evaluate_supported(
        &json!({
            "poly": "(x^2+2)^10*(x^2+10)^2",
            "var": "x",
            "vars": ["x"]
        }),
        "linear_factors",
    )
    .unwrap();
    assert_eq!(
        multiplicity["nonlinear"],
        json!([[2, "10+x^2", 2], [10, "2+x^2", 2]])
    );

    let swapped = evaluate_supported(
        &json!({
            "poly": "(x^2+2)^2*(x^2+10)^10",
            "var": "x",
            "vars": ["x"]
        }),
        "linear_factors",
    )
    .unwrap();
    assert_eq!(
        swapped["nonlinear"],
        json!([[2, "2+x^2", 2], [10, "10+x^2", 2]])
    );

    let ctx = PolyCtx::new(["x", "y"]).unwrap();
    let cubic = wire_poly(&Poly::parse(ctx.clone(), "x^3+y+1").unwrap());
    let quadratic = wire_poly(&Poly::parse(ctx, "x^2+y^2+1").unwrap());
    let mixed_degrees = evaluate_supported(
        &json!({
            "poly": "(x^2+y^2+1)*(x^3+y+1)",
            "var": "x",
            "vars": ["x", "y"]
        }),
        "linear_factors",
    )
    .unwrap();
    assert_eq!(
        mixed_degrees["nonlinear"],
        json!([[1, cubic, 3], [1, quadratic, 2]])
    );
}
