use std::sync::{Arc, Mutex};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use hyperbolica::algebra::{
    DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE, PartialFractionOptions, begin_algebraic_letter_session,
    build_algebraic_letter_atom_list, partial_fractions_factored, partial_fractions_with_options,
};
use hyperbolica::core::{
    FactoredRat, Poly, PolyCtx, Rat, ResultantStrategy, SymCoef, SymCoefSplit, SymMonomial, ZwTable,
};
use hyperbolica::symbols::SYMBOL_NAMESPACE;
use symbolica::prelude::{Integer, Rational, Symbol};

fn core_algebra(criterion: &mut Criterion) {
    let ctx = PolyCtx::new(["x", "y", "z", "s", "t"]).unwrap();

    let mut multiplication = criterion.benchmark_group("polynomial/multiply");
    for power in [4_usize, 8, 12] {
        let left = Poly::parse(ctx.clone(), &format!("(1+x+y+z)^{power}")).unwrap();
        let right = Poly::parse(ctx.clone(), &format!("(1+x-y+s+t)^{power}")).unwrap();
        multiplication.throughput(Throughput::Elements(
            (left.n_terms() * right.n_terms()) as u64,
        ));
        multiplication.bench_with_input(BenchmarkId::from_parameter(power), &power, |bench, _| {
            bench.iter(|| left.try_mul(&right).unwrap())
        });
    }
    multiplication.finish();

    let common = Poly::parse(ctx.clone(), "(x+y+1)^3*(z+s+1)^2").unwrap();
    let left = common
        .try_mul(&Poly::parse(ctx.clone(), "x^3+y*z+s*t+1").unwrap())
        .unwrap();
    let right = common
        .try_mul(&Poly::parse(ctx.clone(), "y^3+x*z-s*t+2").unwrap())
        .unwrap();
    criterion.bench_function("polynomial/gcd", |bench| {
        bench.iter(|| left.gcd(&right).unwrap())
    });

    let resultant_left =
        Poly::parse(ctx.clone(), "x^7+(y+z)*x^5+(s*t+1)*x^3+(y*s-z*t)*x+1").unwrap();
    let resultant_right = Poly::parse(ctx.clone(), "x^6+(y-z)*x^4+(s+t)*x^2+y*t-z*s").unwrap();
    let mut resultants = criterion.benchmark_group("polynomial/resultant_strategies");
    for strategy in [
        ResultantStrategy::Ducos,
        ResultantStrategy::Brown,
        ResultantStrategy::Primitive,
        ResultantStrategy::Crt,
    ] {
        resultants.bench_function(format!("{strategy:?}").to_lowercase(), |bench| {
            bench.iter(|| {
                resultant_left
                    .resultant_with_strategy(&resultant_right, 0, strategy)
                    .unwrap()
            })
        });
    }
    resultants.finish();

    let rational_left = Rat::parse(ctx.clone(), "(x^4+y^2+z*s+1)/((x+y+1)^2*(z+t+1))").unwrap();
    let rational_right = Rat::parse(ctx, "(x^3-y*z+s+2)/((x+y+1)*(z+t+1)^2)").unwrap();
    criterion.bench_function("rational/add_shared_factors", |bench| {
        bench.iter(|| rational_left.try_add(&rational_right).unwrap())
    });
}

fn split_scalar_arithmetic(criterion: &mut Criterion) {
    let wide = PolyCtx::new(["x", "y", "s", "t"]).unwrap();
    let narrow = PolyCtx::new(["x", "y"]).unwrap();
    let table = Arc::new(Mutex::new(ZwTable::new(wide.clone())));

    // Each pair has the same symbolic and W-side key. Wide SymCoef addition
    // therefore performs 24 rational additions/GCD reductions; the split
    // representation only adds the 24 narrow numerator polynomials.
    let mut left_terms = Vec::new();
    let mut right_terms = Vec::new();
    for index in 1..=24 {
        let mut left = SymMonomial::new(
            Rat::parse(wide.clone(), &format!("s*(x^{index}+y+1)/(s+1)")).unwrap(),
        );
        left.log_powers.insert(index + 1, 1);
        let mut right = SymMonomial::new(
            Rat::parse(wide.clone(), &format!("s*(y^{index}+x+2)/(s+1)")).unwrap(),
        );
        right.log_powers.insert(index + 1, 1);
        left_terms.push(left);
        right_terms.push(right);
    }
    let left = SymCoef::from_monomials(wide.clone(), left_terms);
    let right = SymCoef::from_monomials(wide, right_terms);
    let split_left = SymCoefSplit::from_symcoef(&left, narrow.clone(), table.clone()).unwrap();
    let split_right = SymCoefSplit::from_symcoef(&right, narrow, table).unwrap();
    assert_eq!(
        split_left
            .try_add(&split_right)
            .unwrap()
            .as_symcoef()
            .unwrap(),
        left.try_add(&right).unwrap()
    );

    let mut group = criterion.benchmark_group("symbolic/add_24_overlapping_keys");
    group.throughput(Throughput::Elements(24));
    group.bench_function("wide_rational_prefactors", |bench| {
        bench.iter(|| left.try_add(&right).unwrap())
    });
    group.bench_function("split_narrow_prefactors", |bench| {
        bench.iter(|| split_left.try_add(&split_right).unwrap())
    });
    group.finish();
}

fn native_rational_arithmetic(criterion: &mut Criterion) {
    let ctx = PolyCtx::new(["x", "y", "z"]).unwrap();
    let mut group = criterion.benchmark_group("rational/native_scaling");

    for degree in [2_usize, 4, 8] {
        let shared = format!("(x+y+1)^{degree}*(x+z+2)^{degree}");
        let left = Rat::parse(ctx.clone(), &format!("(x^{degree}+y*z+1)/({shared})")).unwrap();
        let right = Rat::parse(ctx.clone(), &format!("(y^{degree}-x*z+2)/({shared})")).unwrap();
        let inverse_shaped =
            Rat::parse(ctx.clone(), &format!("({shared})/(y^{degree}-x*z+2)")).unwrap();

        group.bench_with_input(
            BenchmarkId::new("add_shared_denominator", degree),
            &degree,
            |bench, _| bench.iter(|| left.try_add(&right).unwrap()),
        );
        group.bench_with_input(
            BenchmarkId::new("multiply_cross_cancel", degree),
            &degree,
            |bench, _| bench.iter(|| right.try_mul(&inverse_shaped).unwrap()),
        );
    }

    let power_base = Rat::parse(ctx, "(x+y+z+1)/(x-y+2*z+3)").unwrap();
    for exponent in [7_i64, 31, 63] {
        group.bench_with_input(
            BenchmarkId::new("binary_power", exponent),
            &exponent,
            |bench, exponent| bench.iter(|| power_base.pow(*exponent).unwrap()),
        );
    }
    group.finish();
}

fn typed_substitution_and_evaluation(criterion: &mut Criterion) {
    let ctx = PolyCtx::new(["x", "y", "z", "s", "t"]).unwrap();
    let polynomial = Poly::parse(
        ctx.clone(),
        "3*x^60000*y^7+5*x^4096*z^3-11*x^127*s*t+13*y^31+17",
    )
    .unwrap();
    let rational = Rat::parse(ctx.clone(), "(x^31+y^17*z+3*s*t+1)/((x+y+1)^4*(z+t+2)^3)").unwrap();
    let rational_replacement = Rat::parse(ctx.clone(), "(y^5-z+1)/(s+t+2)").unwrap();
    let half = Rational::new(1, 2);
    let rational_point = [
        Rational::new(1, 2),
        Rational::new(-2, 3),
        Rational::new(3, 5),
        Rational::from(7),
        Rational::new(-1, 11),
    ];
    let integer_point = [1, -2, 3, 7, -1].map(Integer::from);

    let mut group = criterion.benchmark_group("polynomial/typed_exact_ops");
    group.bench_function("sparse_coefficient_60000", |bench| {
        bench.iter(|| polynomial.coefficient_of(0, 60_000).unwrap())
    });
    group.bench_function("substitute_rational", |bench| {
        bench.iter(|| polynomial.substitute_rational(0, &half).unwrap())
    });
    group.bench_function("evaluate_rational", |bench| {
        bench.iter(|| polynomial.evaluate_rational(&rational_point).unwrap())
    });
    group.bench_function("integrate_sparse", |bench| {
        bench.iter(|| polynomial.integrate(0).unwrap())
    });
    group.finish();

    let mut group = criterion.benchmark_group("rational/typed_exact_ops");
    group.bench_function("substitute_rational_native_storage", |bench| {
        bench.iter(|| rational.substitute_rational(0, &half).unwrap())
    });
    group.bench_function("substitute_rat_native_horner", |bench| {
        bench.iter(|| rational.substitute_rat(0, &rational_replacement).unwrap())
    });
    group.bench_function("evaluate_rational_native_storage", |bench| {
        bench.iter(|| rational.evaluate_rational(&rational_point).unwrap())
    });
    group.bench_function("evaluate_integer_native_storage", |bench| {
        bench.iter(|| rational.evaluate_integer(&integer_point).unwrap())
    });
    group.finish();
}

fn algebraic_partial_fractions(criterion: &mut Criterion) {
    let variables = ["bench_alg_x", "bench_alg_a", "bench_alg_b", "bench_alg_z"]
        .into_iter()
        .map(|name| Symbol::parse(name, SYMBOL_NAMESPACE).unwrap().to_atom());
    let ctx = PolyCtx::from_indeterminates(build_algebraic_letter_atom_list(
        variables,
        DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE,
    ))
    .unwrap();
    let _session = begin_algebraic_letter_session().unwrap();
    let options = PartialFractionOptions {
        introduce_algebraic_letters: true,
        forbidden_variables: &[],
    };
    let fixtures = [
        ("simple_quadratic", "1/(bench_alg_x^2-bench_alg_a)"),
        (
            "repeated_quadratic",
            "(bench_alg_x^3+bench_alg_b)/((bench_alg_x^2-bench_alg_a)^4)",
        ),
        (
            "mixed_linear_quadratic",
            "(bench_alg_x^5+bench_alg_a*bench_alg_x+1)/((bench_alg_z*bench_alg_x^2+bench_alg_x+bench_alg_a)^2*(bench_alg_x-bench_alg_b)^3)",
        ),
    ];
    let mut group = criterion.benchmark_group("partial_fractions/algebraic_letters");
    for (name, expression) in fixtures {
        let function = Rat::parse(ctx.clone(), expression).unwrap();
        group.bench_function(name, |bench| {
            bench.iter(|| partial_fractions_with_options(&function, 0, &options).unwrap())
        });
    }
    group.finish();
}

fn factored_denominator_partial_fractions(criterion: &mut Criterion) {
    let ctx = PolyCtx::new(["x", "a", "b", "c"]).unwrap();
    let mut factored = FactoredRat::from_poly(
        Poly::parse(ctx.clone(), "x^13+a*x^11+b*x^8+c*x^5+a*b*x^3+b*c*x+a*b*c+1").unwrap(),
    );
    for (base, exponent) in [
        ("2*x-a", 5),
        ("x+b", 4),
        ("3*x+c", 4),
        ("x+a+b+1", 3),
        ("2*x+b+c+3", 2),
        ("x+a+c+5", 1),
    ] {
        factored
            .push_factor(&Poly::parse(ctx.clone(), base).unwrap(), exponent)
            .unwrap();
    }
    let mut group = criterion.benchmark_group("partial_fractions/factored_denominator");
    group.throughput(Throughput::Elements(factored.den_factors().len() as u64));
    group.bench_function("symbolica_blockwise_components", |bench| {
        bench.iter(|| partial_fractions_factored(&factored, 0).unwrap())
    });
    group.bench_function("eager_materialize_then_canonical", |bench| {
        bench.iter(|| {
            let materialized = factored.materialize().unwrap();
            hyperbolica::algebra::partial_fractions(&materialized, 0).unwrap()
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    core_algebra,
    split_scalar_arithmetic,
    native_rational_arithmetic,
    typed_substitution_and_evaluation,
    algebraic_partial_fractions,
    factored_denominator_partial_fractions
);
criterion_main!(benches);
