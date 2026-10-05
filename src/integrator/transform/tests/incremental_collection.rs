use super::*;

#[test]
fn symbolic_collection_preserves_cancellation_and_order_across_owned_batches() {
    use super::super::collection::{RegulatorSymCollector, collect_regulator_sym_with_digest};
    let ctx = context();
    let first_key = vec![word(&ctx, &["1"]), word(&ctx, &["2"])];
    let second_key = vec![word(&ctx, &["3"])];
    let pi = SymCoef::pi_factor(ctx.clone());
    let term = |coef, key| RegTermSym { coef, key };
    let mut reversed = first_key.clone();
    reversed.reverse();
    reversed.push(Word::default());
    let input = vec![
        term(pi.try_mul_rat(&rat(&ctx, "x+1")).unwrap(), reversed),
        term(SymCoef::from_rat(&rat(&ctx, "3")), second_key.clone()),
        term(
            pi.try_mul_rat(&rat(&ctx, "-x-1")).unwrap(),
            first_key.clone(),
        ),
        term(SymCoef::from_rat(&rat(&ctx, "4")), Vec::new()),
        term(pi.try_mul_rat(&rat(&ctx, "5")).unwrap(), first_key.clone()),
        term(SymCoef::from_rat(&rat(&ctx, "7")), second_key.clone()),
        term(SymCoef::from_rat(&rat(&ctx, "-4")), Vec::new()),
    ];
    let expected = vec![
        term(
            pi.try_mul_rat(&rat(&ctx, "5")).unwrap(),
            canonicalize_regkey(&first_key),
        ),
        term(SymCoef::from_rat(&rat(&ctx, "10")), second_key),
    ];
    // Exercise full equality checks even when every key hashes to one bucket.
    assert_eq!(
        collect_regulator_sym_with_digest(&input, |_| 0).unwrap(),
        expected
    );
    for batch_size in [1, 2, 3, input.len()] {
        let mut collector = RegulatorSymCollector::default();
        for batch in input.chunks(batch_size) {
            collector.extend(batch.to_vec()).unwrap();
        }
        assert_eq!(collector.finish().unwrap(), expected);
    }
    assert!(
        RegulatorSymCollector::default()
            .finish()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn owned_symbolic_collection_checks_coefficient_and_word_contexts() {
    use super::super::collection::{ParallelRegulatorSymCollector, RegulatorSymCollector};
    let ctx = context();
    let foreign = namespaced_context("owned_regulator_foreign");
    for invalid in [
        RegTermSym {
            coef: SymCoef::zero(foreign.clone()),
            key: Vec::new(),
        },
        RegTermSym {
            coef: SymCoef::one(ctx.clone()),
            key: vec![Word::from(vec![generator(&foreign)])],
        },
    ] {
        let mut parallel = ParallelRegulatorSymCollector::new(4);
        parallel
            .extend(vec![RegTermSym {
                coef: SymCoef::one(ctx.clone()),
                key: Vec::new(),
            }])
            .unwrap();
        assert!(matches!(
            parallel.extend(vec![invalid.clone()]),
            Err(Error::ContextMismatch)
        ));
        let mut collector = RegulatorSymCollector::default();
        collector
            .extend(vec![RegTermSym {
                coef: SymCoef::one(ctx.clone()),
                key: Vec::new(),
            }])
            .unwrap();
        assert!(matches!(
            collector.extend(vec![invalid]),
            Err(Error::ContextMismatch)
        ));
    }
}

#[test]
fn parallel_collection_preserves_key_order_balanced_sums_and_hash_collisions() {
    use super::super::collection::{ParallelRegulatorSymCollector, collect_regulator_sym};
    let ctx = context();
    let coefficients = (-3..=3)
        .map(|i| SymCoef::from_rat(&rat(&ctx, &format!("{i}/(x+1)"))))
        .collect::<Vec<_>>();
    let keys = (0..17)
        .map(|i| vec![word(&ctx, &[&i.to_string()]), word(&ctx, &["x"])])
        .collect::<Vec<_>>();
    let input = (0..777)
        .map(|i| {
            let mut key = keys[(i * 7) % keys.len()].clone();
            if i % 2 == 0 {
                key.reverse();
                key.push(Word::default());
            }
            RegTermSym {
                coef: coefficients[i % coefficients.len()].clone(),
                key,
            }
        })
        .collect::<Vec<_>>();
    let expected = collect_regulator_sym(&input).unwrap();
    for workers in [1, 2, 8] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        pool.install(|| {
            for batch in [1, 257, input.len()] {
                let mut collector = ParallelRegulatorSymCollector::new(workers);
                for terms in input.chunks(batch) {
                    collector.extend(terms.to_vec()).unwrap();
                }
                assert_eq!(collector.finish().unwrap(), expected);
            }
            let mut collisions = ParallelRegulatorSymCollector::new(workers);
            collisions.extend_with_digest(input.clone(), |_| 0).unwrap();
            assert_eq!(collisions.finish().unwrap(), expected);
            assert!(
                ParallelRegulatorSymCollector::new(workers)
                    .finish()
                    .unwrap()
                    .is_empty()
            );
        });
    }
}
