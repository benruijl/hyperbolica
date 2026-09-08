use std::cell::Cell;
use std::collections::HashMap;
use std::hash::Hash;

use symbolica::prelude::{AtomCore, Rational};

use crate::core::{Poly, structural_bucket_digest_by};

use super::random::DeterministicRng;
use super::staircase::{ChiCount, ChiStatus};
use super::system::chi_count_sectors;

const UNUSABLE_GENERIC: Option<u64> = None;

/// Per-group memo for generic marginal Euler counts.
#[derive(Clone, Debug, Default)]
pub struct ChiFilterCache {
    generic: HashMap<MarginalKey, Option<u64>>,
}

/// Collision-safe semantic identity of one charted marginal.
///
/// The stable FNV digest remains useful for deterministic seed derivation, but
/// a digest is never used as an equality proof for memoized mathematics.
#[derive(Clone, Debug, Eq, PartialEq)]
struct MarginalKey {
    canonical_factors: Vec<Poly>,
    propagators: Vec<usize>,
    generic_seed: u64,
}

impl std::hash::Hash for MarginalKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.canonical_factors.len().hash(state);
        if let Some(first) = self.canonical_factors.first() {
            first.ctx().native_variables().hash(state);
        }
        for factor in &self.canonical_factors {
            // Marginals normally share one context. Full derived equality
            // still resolves a benign hash collision in a malformed mixed
            // slice whose later factor uses another context.
            factor.hash_canonical_payload(state);
        }
        self.propagators.hash(state);
        self.generic_seed.hash(state);
    }
}

/// Per-request utility counters for the optional Euler filter.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChiFilterStats {
    pub judged: u64,
    pub dropped: u64,
    pub boundary_exempt: u64,
    pub chi_calls: u64,
    /// Conservative abstentions caused by unusable generic or constrained
    /// counts. Such an abstention always keeps a letter.
    pub failure_abstentions: u64,
}

// LR filtering runs sequentially on its caller's thread. Process-global
// counters let another concurrent request reset or overwrite these results.
thread_local! {
    static STATS: Cell<ChiFilterStats> = const { Cell::new(ChiFilterStats {
        judged: 0, dropped: 0, boundary_exempt: 0, chi_calls: 0, failure_abstentions: 0,
    }) };
}

/// Counters for the calling thread since its last reset.
pub fn chi_filter_stats() -> ChiFilterStats {
    STATS.get()
}

pub fn reset_chi_filter_stats() {
    STATS.set(ChiFilterStats::default());
}

fn update_stats(update: impl FnOnce(&mut ChiFilterStats)) {
    let mut stats = STATS.get();
    update(&mut stats);
    STATS.set(stats);
}

fn counted_chi(
    factors: &[Poly],
    exponents: &[i64],
    propagators: &[usize],
    parameters: &[usize],
    constraint: Option<&Poly>,
    seed: u64,
) -> ChiCount {
    update_stats(|stats| stats.chi_calls = stats.chi_calls.wrapping_add(1));
    chi_count_sectors(
        factors,
        exponents,
        propagators,
        parameters,
        constraint,
        seed,
    )
}

/// Namespace-complete, process-stable digest for deterministic sampling only.
///
/// This value is never a cache identity or an equality proof. Mathematical
/// memoization uses the complete [`MarginalKey`] and therefore confirms full
/// context-sensitive polynomial equality. Symbolica's canonical Atom string
/// is namespace-complete and independent of symbol registration order, so it
/// avoids both presentation aliases from [`crate::core::PolyCtx::vars`] and
/// process-local symbol ids. Serialization is confined to seed construction;
/// no lookup or CAS operation reparses it.
fn sampling_digest(domain: u64, factors: &[Poly], indices: &[usize]) -> u64 {
    structural_bucket_digest_by(domain, |state| {
        factors.len().hash(state);
        if let Some(first) = factors.first() {
            first.ctx().len().hash(state);
            for variable in 0..first.ctx().len() {
                let atom = first
                    .ctx()
                    .variable_atom(variable)
                    .expect("a context index below its length must exist");
                atom.to_canonical_string().hash(state);
            }
        }
        for factor in factors {
            debug_assert!(
                factors
                    .first()
                    .is_none_or(|first| factor.ctx().is_compatible_with(first.ctx()))
            );
            factor.inner().coefficients.hash(state);
            factor.inner().exponents.hash(state);
        }
        indices.hash(state);
    })
}

fn homogeneous_in(polynomial: &Poly, variables: &[usize]) -> bool {
    let mut degrees = polynomial.inner().exponents_iter().map(|exponents| {
        variables
            .iter()
            .map(|variable| u64::from(exponents[*variable]))
            .sum::<u64>()
    });
    let Some(first) = degrees.next() else {
        return true;
    };
    degrees.all(|degree| degree == first)
}

fn is_boundary_monomial(polynomial: &Poly) -> bool {
    // Upstream's `MemberQ[allVars, letter]` exemption intentionally covers a
    // scalar multiple of any context variable, not only the active subset.
    polynomial.n_terms() == 1
        && polynomial
            .inner()
            .exponents(0)
            .iter()
            .copied()
            .map(u64::from)
            .sum::<u64>()
            == 1
}

fn small_is_prime(value: u64) -> bool {
    if value < 2 {
        return false;
    }
    if value.is_multiple_of(2) {
        return value == 2;
    }
    let mut divisor = 3_u64;
    while divisor <= value / divisor {
        if value.is_multiple_of(divisor) {
            return false;
        }
        divisor += 2;
    }
    true
}

fn twist_exponents(count: usize, seed: u64) -> Option<Vec<i64>> {
    // Match the upstream 64-prime pool and fail conservatively if an unusual
    // augmented face exceeds it instead of silently introducing zero twists.
    if count > 64 {
        return None;
    }
    let mut pool = Vec::with_capacity(64);
    let mut candidate = 127_u64;
    while pool.len() < 64 {
        if small_is_prime(candidate) {
            pool.push(i64::try_from(candidate).ok()?);
        }
        candidate += 1;
    }
    let mut rng = DeterministicRng::new(seed);
    rng.shuffle(&mut pool);
    pool.truncate(count);
    Some(pool)
}

fn finite_generic(count: ChiCount) -> Option<u64> {
    (count.status == ChiStatus::Finite).then_some(count.count)
}

/// Decide whether one LR letter is a genuine singular locus.
///
/// A destructive (fictitious-letter) verdict requires two independent
/// constrained draws that both fail to lower the generic count. Every native
/// algebra failure and every positive-dimensional constrained system keeps the
/// letter.
pub fn chi_letter_genuine(
    augmented_group: &[Poly],
    subset_variable_indices: &[usize],
    letter: &Poly,
    cache: &mut ChiFilterCache,
    base_seed: u64,
) -> bool {
    if augmented_group.is_empty() || subset_variable_indices.is_empty() {
        return true;
    }
    if augmented_group
        .iter()
        .any(|factor| !factor.ctx().is_compatible_with(letter.ctx()))
        || subset_variable_indices
            .iter()
            .any(|variable| *variable >= letter.ctx().len())
    {
        update_stats(|stats| stats.failure_abstentions = stats.failure_abstentions.wrapping_add(1));
        return true;
    }

    // Quotient the common homogeneous C* scaling by charting the final subset
    // variable, exactly as the reference auto-chart.
    let mut propagators = subset_variable_indices.to_vec();
    let mut factors = augmented_group.to_vec();
    if propagators.len() >= 2
        && factors
            .iter()
            .all(|factor| homogeneous_in(factor, &propagators))
    {
        let chart = propagators.pop().expect("nonempty chart variables");
        let mut charted = Vec::with_capacity(factors.len());
        for factor in factors {
            let Ok(value) = factor.substitute_rational(chart, &Rational::one()) else {
                update_stats(|stats| {
                    stats.failure_abstentions = stats.failure_abstentions.wrapping_add(1)
                });
                return true;
            };
            charted.push(value);
        }
        factors = charted;
    }

    let mut canonical_factors = Vec::with_capacity(factors.len());
    for factor in &mut factors {
        *factor = factor.canonical_proportional_form();
        canonical_factors.push(factor.clone());
    }
    let seed_digest = sampling_digest(0x4555_4c45_5246_4143, &canonical_factors, &propagators);
    let twist_seed = base_seed ^ seed_digest;
    let marginal_key = MarginalKey {
        canonical_factors,
        propagators: propagators.clone(),
        generic_seed: twist_seed,
    };
    let Some(exponents) = twist_exponents(factors.len(), twist_seed) else {
        update_stats(|stats| stats.failure_abstentions = stats.failure_abstentions.wrapping_add(1));
        return true;
    };
    let parameters = (0..letter.ctx().len())
        .filter(|variable| !propagators.contains(variable))
        .collect::<Vec<_>>();

    let generic = if let Some(cached) = cache.generic.get(&marginal_key) {
        *cached
    } else {
        let first = finite_generic(counted_chi(
            &factors,
            &exponents,
            &propagators,
            &parameters,
            None,
            twist_seed.wrapping_add(1),
        ));
        let second = finite_generic(counted_chi(
            &factors,
            &exponents,
            &propagators,
            &parameters,
            None,
            twist_seed.wrapping_add(2),
        ));
        let value = match (first, second) {
            (Some(left), Some(right)) => Some(left.max(right)),
            (Some(value), None) | (None, Some(value)) => Some(value),
            (None, None) => UNUSABLE_GENERIC,
        };
        cache.generic.insert(marginal_key, value);
        value
    };
    let Some(generic) = generic else {
        update_stats(|stats| stats.failure_abstentions = stats.failure_abstentions.wrapping_add(1));
        return true;
    };

    let constraint = letter.canonical_proportional_form();
    let draw_seed = twist_seed
        ^ sampling_digest(
            0x4555_4c45_5243_4f4e,
            std::slice::from_ref(&constraint),
            &[],
        );
    let first = counted_chi(
        &factors,
        &exponents,
        &propagators,
        &parameters,
        Some(&constraint),
        draw_seed.wrapping_add(3),
    );
    match first.status {
        ChiStatus::PositiveDim | ChiStatus::Failed => {
            update_stats(|stats| {
                stats.failure_abstentions = stats.failure_abstentions.wrapping_add(1)
            });
            return true;
        }
        ChiStatus::Finite if first.count < generic => return true,
        ChiStatus::Finite => {}
    }

    let second = counted_chi(
        &factors,
        &exponents,
        &propagators,
        &parameters,
        Some(&constraint),
        draw_seed.wrapping_add(4),
    );
    match second.status {
        ChiStatus::Finite => second.count < generic,
        ChiStatus::PositiveDim | ChiStatus::Failed => {
            update_stats(|stats| {
                stats.failure_abstentions = stats.failure_abstentions.wrapping_add(1)
            });
            true
        }
    }
}

/// Filter one subset-table letter list, exempting domain-boundary monomials.
pub fn chi_filter_letters(
    augmented_group: &[Poly],
    subset_variable_indices: &[usize],
    letters: &[Poly],
    cache: &mut ChiFilterCache,
    base_seed: u64,
) -> Vec<Poly> {
    let mut kept = Vec::with_capacity(letters.len());
    for letter in letters {
        if is_boundary_monomial(letter) {
            update_stats(|stats| stats.boundary_exempt = stats.boundary_exempt.wrapping_add(1));
            kept.push(letter.clone());
            continue;
        }
        update_stats(|stats| stats.judged = stats.judged.wrapping_add(1));
        if chi_letter_genuine(
            augmented_group,
            subset_variable_indices,
            letter,
            cache,
            base_seed,
        ) {
            kept.push(letter.clone());
        } else {
            update_stats(|stats| stats.dropped = stats.dropped.wrapping_add(1));
        }
    }
    kept
}

#[cfg(test)]
#[path = "filter/seed_tests.rs"]
mod seed_tests;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use symbolica::prelude::Symbol;

    use crate::core::PolyCtx;

    use super::*;

    fn parse(context: &Arc<PolyCtx>, expression: &str) -> Poly {
        Poly::parse(context.clone(), expression).unwrap()
    }

    fn box_group(context: &Arc<PolyCtx>) -> Vec<Poly> {
        ["x1+x2+x3+x4+s*x1*x3+t*x2*x4", "x1", "x2", "x3", "x4"]
            .map(|expression| parse(context, expression))
            .to_vec()
    }

    #[test]
    fn lee_pomeransky_box_verdicts_and_cache() {
        let context = PolyCtx::new(["x1", "x2", "x3", "x4", "s", "t"]).unwrap();
        let group = box_group(&context);
        let mut cache = ChiFilterCache::default();
        for genuine in ["s", "t", "s+t"] {
            assert!(chi_letter_genuine(
                &group,
                &[0, 1, 2, 3],
                &parse(&context, genuine),
                &mut cache,
                87_178,
            ));
        }
        assert!(!chi_letter_genuine(
            &group,
            &[0, 1, 2, 3],
            &parse(&context, "s+7*t"),
            &mut cache,
            87_178,
        ));
        assert_eq!(cache.generic.len(), 1);
    }

    #[test]
    fn homogeneous_split_pair_auto_chart_matches_box_verdicts() {
        let context = PolyCtx::new(["x1", "x2", "x3", "x4", "s", "t"]).unwrap();
        let group = ["x1+x2+x3+x4", "s*x1*x3+t*x2*x4", "x1", "x2", "x3", "x4"]
            .map(|expression| parse(&context, expression));
        let mut cache = ChiFilterCache::default();
        for genuine in ["s", "t", "s+t"] {
            assert!(chi_letter_genuine(
                &group,
                &[0, 1, 2, 3],
                &parse(&context, genuine),
                &mut cache,
                87_178,
            ));
        }
        assert!(!chi_letter_genuine(
            &group,
            &[0, 1, 2, 3],
            &parse(&context, "s+7*t"),
            &mut cache,
            87_178,
        ));
    }

    #[test]
    fn uq5_intermediate_fixture() {
        let context = PolyCtx::new([
            "x1", "x2", "x3", "x4", "x5", "qq1", "qq2", "wb1", "wb2", "yb",
        ])
        .unwrap();
        let group = [
            "x1+x2+x3",
            "-qq1*x1*x2-qq2*x1*x3+2*wb1*x3*x4-x4^2+2*wb2*x2*x5-x5^2+2*yb*x4*x5",
            "x1",
            "x2",
            "x3",
            "x4",
            "x5",
        ]
        .map(|expression| parse(&context, expression));
        let mut cache = ChiFilterCache::default();
        assert!(!chi_letter_genuine(
            &group,
            &[1, 2, 3, 4],
            &parse(&context, "yb"),
            &mut cache,
            87_178,
        ));
        assert!(chi_letter_genuine(
            &group,
            &[1, 2, 3, 4],
            &parse(&context, "wb1^2+wb2^2-2*wb1*wb2*yb"),
            &mut cache,
            87_178,
        ));
    }

    #[test]
    fn boundary_exemption_and_stats_are_exact() {
        let context = PolyCtx::new(["x1", "x2", "x3", "x4", "s", "t"]).unwrap();
        let group = box_group(&context);
        let letters = [parse(&context, "x3"), parse(&context, "s+1")];
        let mut cache = ChiFilterCache::default();
        reset_chi_filter_stats();
        let kept = chi_filter_letters(&group, &[0, 1], &letters, &mut cache, 87_178);
        assert!(kept.iter().any(|letter| letter.equal(&letters[0])));
        let stats = chi_filter_stats();
        assert_eq!(stats.boundary_exempt, 1);
        assert_eq!(stats.judged, 1);
    }

    #[test]
    fn marginal_cache_never_uses_a_folded_mask_as_identity() {
        let context = PolyCtx::new(["f"]).unwrap();
        let left = MarginalKey {
            canonical_factors: vec![parse(&context, "f")],
            propagators: vec![0],
            generic_seed: 17,
        };
        let right = MarginalKey {
            canonical_factors: vec![parse(&context, "f")],
            propagators: vec![63],
            generic_seed: 17,
        };
        // These positions collide in the legacy seed mask, but must remain
        // distinct mathematical marginals in the cache.
        let folded = |variable: usize| 1_u64 << (variable % 63);
        assert_eq!(folded(0), folded(63));
        assert_ne!(left, right);
        let mut cache = HashMap::new();
        cache.insert(left, Some(1));
        cache.insert(right, Some(2));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn marginal_identity_preserves_symbol_namespaces() {
        let left_symbol = Symbol::parse("x", "euler_key_left").unwrap();
        let right_symbol = Symbol::parse("x", "euler_key_right").unwrap();
        let left_context = PolyCtx::from_symbols([left_symbol]).unwrap();
        let right_context = PolyCtx::from_symbols([right_symbol]).unwrap();
        let left = MarginalKey {
            canonical_factors: vec![Poly::generator(left_context, 0).unwrap()],
            propagators: vec![0],
            generic_seed: 17,
        };
        let right = MarginalKey {
            canonical_factors: vec![Poly::generator(right_context, 0).unwrap()],
            propagators: vec![0],
            generic_seed: 17,
        };

        assert_ne!(left, right);
        let mut cache = HashMap::new();
        cache.insert(left, Some(1));
        cache.insert(right, Some(2));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn sampling_seed_uses_native_identity_not_diagnostic_aliases() {
        let left_symbol = Symbol::parse("x", "euler_seed_left").unwrap();
        let right_symbol = Symbol::parse("x", "euler_seed_right").unwrap();
        let left_context = PolyCtx::from_symbols([left_symbol]).unwrap();
        let equivalent_context = PolyCtx::from_indeterminates([left_symbol.to_atom()]).unwrap();
        let right_context = PolyCtx::from_symbols([right_symbol]).unwrap();

        let left = Poly::generator(left_context, 0).unwrap();
        let equivalent = Poly::generator(equivalent_context, 0).unwrap();
        let right = Poly::generator(right_context, 0).unwrap();

        assert_eq!(
            sampling_digest(17, std::slice::from_ref(&left), &[0]),
            sampling_digest(17, std::slice::from_ref(&equivalent), &[0])
        );
        assert_ne!(
            sampling_digest(17, std::slice::from_ref(&left), &[0]),
            sampling_digest(17, std::slice::from_ref(&right), &[0])
        );
    }

    #[test]
    fn invalid_constraint_abstains_conservatively() {
        let context = PolyCtx::new(["x", "s"]).unwrap();
        let group = [parse(&context, "1+x+s*x")];
        let mut cache = ChiFilterCache::default();
        reset_chi_filter_stats();
        // A constraint involving the active propagator violates the kinematic
        // constraint contract. The filter must keep it, never turn failure
        // into a destructive verdict.
        assert!(chi_letter_genuine(
            &group,
            &[0],
            &parse(&context, "x+s"),
            &mut cache,
            87_178,
        ));
        assert!(chi_filter_stats().failure_abstentions >= 1);
    }
}
