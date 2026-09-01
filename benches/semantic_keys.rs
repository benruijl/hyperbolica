use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use hyperbolica::core::{PolyCtx, Rat};
use hyperbolica::integrator::{RegTerm, ShuffleEntry, collect_regulator, integration_step};
use hyperbolica::reduce::{MzvReductionTable, fibration_basis};
use hyperbolica::symbols::Word;

fn semantic_keys(criterion: &mut Criterion) {
    let ctx = PolyCtx::new(["x"]).unwrap();
    let regulator = (0..512)
        .map(|index| RegTerm {
            coef: Rat::one(ctx.clone()),
            key: vec![Word::from(vec![Rat::from_int(
                ctx.clone(),
                i64::from(index % 64) + 1,
            )])],
        })
        .collect::<Vec<_>>();

    let mut group = criterion.benchmark_group("semantic_keys/structural_buckets");
    group.sample_size(10);
    group.throughput(Throughput::Elements(regulator.len() as u64));
    group.bench_function("collect_regulator_512_to_64", |bench| {
        bench.iter(|| collect_regulator(black_box(&regulator)).unwrap())
    });

    let period_input = (0..512)
        .map(|_| RegTerm {
            coef: Rat::one(ctx.clone()),
            key: Vec::new(),
        })
        .collect::<Vec<_>>();
    let table = MzvReductionTable::default();
    group.bench_function("fibration_accumulate_512_to_1", |bench| {
        bench.iter(|| {
            fibration_basis(
                black_box(&ctx),
                black_box(&period_input),
                black_box(&[]),
                black_box(&table),
            )
            .unwrap()
        })
    });

    let integration_input = (0..32)
        .map(|_| ShuffleEntry::new(Rat::parse(ctx.clone(), "1/(x+1)^2").unwrap(), Vec::new()))
        .collect::<Vec<_>>();
    group.throughput(Throughput::Elements(integration_input.len() as u64));
    group.bench_function("integration_step_32_shared_spines", |bench| {
        bench.iter(|| {
            integration_step(
                black_box(&ctx),
                black_box(&integration_input),
                0,
                black_box(&table),
                false,
            )
            .unwrap()
        })
    });
    group.finish();
}

criterion_group!(benches, semantic_keys);
criterion_main!(benches);
