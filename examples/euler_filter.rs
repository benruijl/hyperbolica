use hyperbolica::algebra::euler::{
    ChiFilterCache, chi_filter_letters, chi_filter_stats, chi_system_timings,
    reset_chi_filter_stats, reset_chi_system_timings,
};
use hyperbolica::core::{Poly, PolyCtx};

fn main() -> hyperbolica::Result<()> {
    let context = PolyCtx::new(["x1", "x2", "x3", "x4", "s", "t"])?;
    let group = ["x1+x2+x3+x4+s*x1*x3+t*x2*x4", "x1", "x2", "x3", "x4"]
        .into_iter()
        .map(|expression| Poly::parse(context.clone(), expression))
        .collect::<hyperbolica::Result<Vec<_>>>()?;
    let letters = ["s", "t", "s+t", "s+7*t"]
        .into_iter()
        .map(|expression| Poly::parse(context.clone(), expression))
        .collect::<hyperbolica::Result<Vec<_>>>()?;

    reset_chi_filter_stats();
    reset_chi_system_timings();
    let kept = chi_filter_letters(
        &group,
        &[0, 1, 2, 3],
        &letters,
        &mut ChiFilterCache::default(),
        87_178,
    );
    println!(
        "kept letters: {:?}",
        kept.iter().map(ToString::to_string).collect::<Vec<_>>()
    );
    let stats = chi_filter_stats();
    let timings = chi_system_timings();
    println!(
        "judged={} dropped={} chi_calls={} systems={} construction={:?} f4={:?}",
        stats.judged,
        stats.dropped,
        stats.chi_calls,
        timings.systems,
        timings.construction,
        timings.f4,
    );
    Ok(())
}
