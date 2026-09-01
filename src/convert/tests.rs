use super::{expr::Expr, parse_expression};

fn max_log_weight(expression: &Expr) -> usize {
    match expression {
        Expr::Leaf(_) => 0,
        Expr::Plus(children) => children.iter().map(max_log_weight).max().unwrap_or(0),
        Expr::Times(children) => children.iter().map(max_log_weight).sum(),
        Expr::Power(base, exponent) => max_log_weight(base) * usize::try_from(*exponent).unwrap(),
        Expr::Hlog { word, .. } => word.letters.len(),
    }
}

#[test]
fn legacy_bridge_ingests_the_complete_public_smirnov_corpus() {
    let smirnov = [
        (
            "tst0",
            include_str!("../../tests/data/smirnov/tst0.txt"),
            177,
            0,
        ),
        (
            "tst1",
            include_str!("../../tests/data/smirnov/tst1.txt"),
            531,
            1,
        ),
        (
            "tst2",
            include_str!("../../tests/data/smirnov/tst2.txt"),
            3_232,
            2,
        ),
        (
            "tst3",
            include_str!("../../tests/data/smirnov/tst3.txt"),
            14_106,
            3,
        ),
        (
            "tst4",
            include_str!("../../tests/data/smirnov/tst4.txt"),
            41_327,
            4,
        ),
    ];
    let variables = ["t1", "t2", "t3", "t4", "t5"].map(str::to_owned);

    for (name, source, byte_len, weight) in smirnov {
        assert_eq!(source.len(), byte_len, "fixture drift in {name}");
        let parsed = parse_expression(source, &variables, true)
            .unwrap_or_else(|error| panic!("failed to ingest {name}: {error}"));
        assert_eq!(parsed.augmented_vars, variables, "variable drift in {name}");
        assert_eq!(
            max_log_weight(&parsed.expr),
            weight,
            "weight drift in {name}"
        );
    }
}
