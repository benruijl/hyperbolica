use criterion::{Criterion, criterion_group, criterion_main};
use hyperbolica::algebra::euler::{chi_count_sectors, reset_chi_system_timings};
use hyperbolica::core::{Poly, PolyCtx};

fn native_euler(criterion: &mut Criterion) {
    let context = PolyCtx::new(["x1", "x2", "x3", "x4", "s", "t"]).unwrap();
    let factor = Poly::parse(context.clone(), "x1+x2+x3+x4+s*x1*x3+t*x2*x4").unwrap();
    let fake = Poly::parse(context, "s+7*t").unwrap();

    let mut group = criterion.benchmark_group("euler/native_symbolica");
    group.sample_size(10);
    group.bench_function("box_generic_16_sectors", |bench| {
        bench.iter(|| {
            reset_chi_system_timings();
            chi_count_sectors(
                std::slice::from_ref(&factor),
                &[101],
                &[0, 1, 2, 3],
                &[4, 5],
                None,
                20_260_604,
            )
        })
    });
    group.bench_function("box_constrained_16_sectors", |bench| {
        bench.iter(|| {
            reset_chi_system_timings();
            chi_count_sectors(
                std::slice::from_ref(&factor),
                &[101],
                &[0, 1, 2, 3],
                &[4, 5],
                Some(&fake),
                20_260_607,
            )
        })
    });
    group.finish();
}

criterion_group!(benches, native_euler);
criterion_main!(benches);
