use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use hyperbolica::core::{Poly, PolyCtx};
use hyperbolica::integrator::factor_table::{FactorTableLimits, factor_table};
use hyperbolica::integrator::lr_search::{dedup_proportional, intersect_proportional};

fn lr_structural_keys(criterion: &mut Criterion) {
    let ctx = PolyCtx::new(["x", "y", "z", "s"]).unwrap();
    let mut duplicated = Vec::new();
    let mut first = Vec::new();
    let mut second = Vec::new();
    let mut third = Vec::new();
    let mut fourth = Vec::new();
    for index in 1..=128 {
        let base = format!("x+{index}*y+z+s+1");
        first.push(Poly::parse(ctx.clone(), &base).unwrap());
        second.push(Poly::parse(ctx.clone(), &format!("2*({base})")).unwrap());
        third.push(Poly::parse(ctx.clone(), &format!("-3*({base})")).unwrap());
        fourth.push(Poly::parse(ctx.clone(), &format!("5*({base})")).unwrap());
        duplicated.push(Poly::parse(ctx.clone(), &base).unwrap());
        duplicated.push(Poly::parse(ctx.clone(), &format!("7*({base})")).unwrap());
        duplicated.push(Poly::parse(ctx.clone(), &format!("-11*({base})")).unwrap());
        duplicated.push(Poly::parse(ctx.clone(), &format!("13*({base})")).unwrap());
    }

    let mut group = criterion.benchmark_group("lr/structural_keys");
    group.throughput(Throughput::Elements(duplicated.len() as u64));
    group.bench_function("proportional_dedup_512_to_128", |bench| {
        bench.iter(|| dedup_proportional(black_box(&duplicated)))
    });
    let lists = vec![first, second, third, fourth];
    group.throughput(Throughput::Elements(
        lists.iter().map(Vec::len).sum::<usize>() as u64,
    ));
    group.bench_function("proportional_intersection_4x128", |bench| {
        bench.iter(|| intersect_proportional(black_box(&lists)))
    });
    group.finish();

    let factor_group = (1..=10)
        .map(|index| Poly::parse(ctx.clone(), &format!("x+{index}*y+z+s")).unwrap())
        .collect::<Vec<_>>();
    let mut group = criterion.benchmark_group("factor_table/structural_interner");
    group.sample_size(10);
    group.throughput(Throughput::Elements(factor_group.len() as u64));
    group.bench_function("ten_linear_letters", |bench| {
        bench.iter(|| {
            factor_table(
                black_box(std::slice::from_ref(&factor_group)),
                black_box(&[0, 1]),
                false,
                FactorTableLimits::default(),
            )
            .unwrap()
        })
    });
    group.finish();
}

criterion_group!(benches, lr_structural_keys);
criterion_main!(benches);
