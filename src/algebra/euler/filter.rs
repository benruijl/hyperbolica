use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use symbolica::prelude::Rational;

use crate::core::Poly;

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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct MarginalKey {
    canonical_factors: Vec<String>,
    propagators: Vec<usize>,
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

static JUDGED: AtomicU64 = AtomicU64::new(0);
static DROPPED: AtomicU64 = AtomicU64::new(0);
static BOUNDARY_EXEMPT: AtomicU64 = AtomicU64::new(0);
static CHI_CALLS: AtomicU64 = AtomicU64::new(0);
static FAILURE_ABSTENTIONS: AtomicU64 = AtomicU64::new(0);

pub fn chi_filter_stats() -> ChiFilterStats {
    ChiFilterStats {
        judged: JUDGED.load(Ordering::Relaxed),
        dropped: DROPPED.load(Ordering::Relaxed),
        boundary_exempt: BOUNDARY_EXEMPT.load(Ordering::Relaxed),
        chi_calls: CHI_CALLS.load(Ordering::Relaxed),
        failure_abstentions: FAILURE_ABSTENTIONS.load(Ordering::Relaxed),
    }
}

pub fn reset_chi_filter_stats() {
    JUDGED.store(0, Ordering::Relaxed);
    DROPPED.store(0, Ordering::Relaxed);
    BOUNDARY_EXEMPT.store(0, Ordering::Relaxed);
    CHI_CALLS.store(0, Ordering::Relaxed);
    FAILURE_ABSTENTIONS.store(0, Ordering::Relaxed);
}

fn counted_chi(
    factors: &[Poly],
    exponents: &[i64],
    propagators: &[usize],
    parameters: &[usize],
    constraint: Option<&Poly>,
    seed: u64,
) -> ChiCount {
    CHI_CALLS.fetch_add(1, Ordering::Relaxed);
    chi_count_sectors(
        factors,
        exponents,
        propagators,
        parameters,
        constraint,
        seed,
    )
}

fn fnv1a(bytes: &[u8], mut hash: u64) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    hash
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
        .any(|factor| factor.ctx().vars() != letter.ctx().vars())
        || subset_variable_indices
            .iter()
            .any(|variable| *variable >= letter.ctx().len())
    {
        FAILURE_ABSTENTIONS.fetch_add(1, Ordering::Relaxed);
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
                FAILURE_ABSTENTIONS.fetch_add(1, Ordering::Relaxed);
                return true;
            };
            charted.push(value);
        }
        factors = charted;
    }

    let mut face_hash = 1_469_598_103_934_665_603_u64;
    let mut canonical_factors = Vec::with_capacity(factors.len());
    for factor in &mut factors {
        *factor = factor.canonical_proportional_form();
        let canonical = factor.to_string();
        face_hash = fnv1a(canonical.as_bytes(), face_hash);
        canonical_factors.push(canonical);
    }
    let mask_hash = propagators
        .iter()
        .fold(0_u64, |mask, variable| mask | (1_u64 << (variable % 63)));
    let seed_digest = face_hash ^ mask_hash.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let marginal_key = MarginalKey {
        canonical_factors,
        propagators: propagators.clone(),
    };
    let twist_seed = base_seed ^ seed_digest;
    let Some(exponents) = twist_exponents(factors.len(), twist_seed) else {
        FAILURE_ABSTENTIONS.fetch_add(1, Ordering::Relaxed);
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
        FAILURE_ABSTENTIONS.fetch_add(1, Ordering::Relaxed);
        return true;
    };

    let constraint = letter.canonical_proportional_form();
    let draw_seed =
        twist_seed ^ fnv1a(constraint.to_string().as_bytes(), 1_469_598_103_934_665_603);
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
            FAILURE_ABSTENTIONS.fetch_add(1, Ordering::Relaxed);
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
            FAILURE_ABSTENTIONS.fetch_add(1, Ordering::Relaxed);
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
            BOUNDARY_EXEMPT.fetch_add(1, Ordering::Relaxed);
            kept.push(letter.clone());
            continue;
        }
        JUDGED.fetch_add(1, Ordering::Relaxed);
        if chi_letter_genuine(
            augmented_group,
            subset_variable_indices,
            letter,
            cache,
            base_seed,
        ) {
            kept.push(letter.clone());
        } else {
            DROPPED.fetch_add(1, Ordering::Relaxed);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

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
        let left = MarginalKey {
            canonical_factors: vec!["f".into()],
            propagators: vec![0],
        };
        let right = MarginalKey {
            canonical_factors: vec!["f".into()],
            propagators: vec![63],
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
