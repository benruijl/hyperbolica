//! Linear-reducibility search by Brown/Fubini polynomial reduction.
//!
//! This is the Symbolica-backed counterpart of HyperFLINT's
//! `integrator/lr_search.cpp`.  The important detail is the subset table:
//! the letters attached to a subset are the proportional intersection of
//! every one-variable path into that subset.  A single reduction chain is
//! only an over-approximation and can produce false LR obstructions.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::{Duration, Instant};

use symbolica::prelude::*;

use crate::algebra::euler::{ChiFilterCache, chi_filter_letters, reset_chi_filter_stats};
use crate::core::Poly;
use crate::error::{Error, Result};

use super::structural_keys::{PolyIndex, PolySliceCache};

/// Options for [`find_lr_orders`].
#[derive(Clone, Copy, Debug)]
pub struct LrSearchOptions {
    /// Admit quadratic letters in addition to linear letters.
    pub allow_algebraic_letters: bool,
    /// Carry pending square-root obligations to later pivots.
    pub carry_discharge: bool,
    /// Relative score cutoff at each subset size. `INFINITY` is exhaustive.
    pub score_prune_factor: f64,
    /// Apply the pure-Symbolica Euler-characteristic genuineness filter to
    /// each subset-table intersection.
    pub euler_filter: bool,
}

impl Default for LrSearchOptions {
    fn default() -> Self {
        Self {
            allow_algebraic_letters: false,
            carry_discharge: false,
            score_prune_factor: f64::INFINITY,
            euler_filter: false,
        }
    }
}

/// Best order and its HyperFLINT-compatible heuristic profile.
#[derive(Clone, Debug)]
pub struct LrResult {
    /// Original polynomial-context variable indices, in integration order.
    pub order: Vec<usize>,
    pub score: f64,
    /// Quadratic letters encountered along the selected order.
    pub root_polys: Vec<Poly>,
    pub carried_sqrts: u64,
    pub kin_sqrts: u64,
    pub terminal_quads: u64,
    /// Distinct obligations minted along a carry-discharge path.
    pub obligation_polys: Vec<Poly>,
}

impl LrResult {
    fn empty(score: f64) -> Self {
        Self {
            order: Vec::new(),
            score,
            root_polys: Vec::new(),
            carried_sqrts: 0,
            kin_sqrts: 0,
            terminal_quads: 0,
            obligation_polys: Vec::new(),
        }
    }

    /// True when no linearly-reducible order exists.
    pub fn is_nolr(&self) -> bool {
        self.order.is_empty() && !self.score.is_finite()
    }
}

/// Optional side channel for irreducible factors that are independent of all
/// integration variables.
#[derive(Clone, Debug, Default)]
pub struct SingCollector {
    integration_variables: BTreeSet<usize>,
    seen: PolyIndex,
    ordered: Vec<Poly>,
}

impl SingCollector {
    /// Singularities in deterministic first-encounter order.
    ///
    /// Values remain exact structural polynomials until a transport adapter
    /// chooses how to print them.
    pub fn singularities(&self) -> &[Poly] {
        &self.ordered
    }

    fn seed(&mut self, variables: &[usize]) {
        self.integration_variables.clear();
        self.integration_variables.extend(variables.iter().copied());
        self.seen.clear();
        self.ordered.clear();
    }

    fn observe(&mut self, factor: &Poly) {
        if factor.is_zero() || factor.is_rational_constant() {
            return;
        }
        if factor
            .used_variable_indices()
            .iter()
            .any(|variable| self.integration_variables.contains(variable))
        {
            return;
        }
        let canonical = factor.canonical_proportional_form();
        self.seen.insert(&mut self.ordered, canonical);
    }
}

#[derive(Clone, Debug)]
struct Budget {
    started: Instant,
    duration: Option<Duration>,
    max_operand_terms: Option<usize>,
}

impl Budget {
    fn from_environment() -> Result<Self> {
        let duration = match std::env::var("HF_LR_TIME_BUDGET_S") {
            Ok(raw) if !raw.is_empty() && raw != "0" => {
                let seconds = raw.parse::<f64>().map_err(|_| {
                    Error::InvalidInput(format!(
                        "HF_LR_TIME_BUDGET_S must be a non-negative number, got `{raw}`"
                    ))
                })?;
                if !seconds.is_finite() || seconds < 0.0 {
                    return Err(Error::InvalidInput(format!(
                        "HF_LR_TIME_BUDGET_S must be finite and non-negative, got `{raw}`"
                    )));
                }
                (seconds > 0.0).then(|| Duration::from_secs_f64(seconds))
            }
            _ => None,
        };
        let max_operand_terms = match std::env::var("HF_LR_MAX_OPERAND_TERMS") {
            Ok(raw) if !raw.is_empty() && raw != "0" => {
                let value = raw.parse::<usize>().map_err(|_| {
                    Error::InvalidInput(format!(
                        "HF_LR_MAX_OPERAND_TERMS must be a non-negative integer, got `{raw}`"
                    ))
                })?;
                (value > 0).then_some(value)
            }
            _ => None,
        };
        Ok(Self {
            started: Instant::now(),
            duration,
            max_operand_terms,
        })
    }

    fn check_time(&self, where_: &str) -> Result<()> {
        if self
            .duration
            .is_some_and(|duration| self.started.elapsed() >= duration)
        {
            return Err(Error::InvalidInput(format!(
                "LR time budget exceeded at {where_}"
            )));
        }
        Ok(())
    }

    fn check_operand(&self, what: &str, terms: usize) -> Result<()> {
        if self.max_operand_terms.is_some_and(|limit| terms > limit) {
            return Err(Error::InvalidInput(format!(
                "LR operand-size budget exceeded: {what} has {terms} terms"
            )));
        }
        Ok(())
    }

    fn check_pair(&self, what: &str, left: usize, right: usize) -> Result<()> {
        if self
            .max_operand_terms
            .is_some_and(|limit| left.saturating_mul(right) > limit)
        {
            return Err(Error::InvalidInput(format!(
                "LR operand-size budget exceeded: {what} has {left} x {right} terms"
            )));
        }
        Ok(())
    }
}

/// Per-request reduction engine.  Both memo layers are value-preserving and
/// avoid repeatedly factoring the same subset marginals.
pub(crate) struct ReductionEngine {
    step_cache: PolySliceCache<Vec<Poly>>,
    factor_cache: HashMap<Poly, Vec<Poly>>,
    budget: Budget,
}

impl ReductionEngine {
    pub(crate) fn new() -> Result<Self> {
        Ok(Self {
            step_cache: PolySliceCache::default(),
            factor_cache: HashMap::new(),
            budget: Budget::from_environment()?,
        })
    }

    pub(crate) fn check_time(&self, where_: &str) -> Result<()> {
        self.budget.check_time(where_)
    }

    pub(crate) fn check_operand(&self, what: &str, terms: usize) -> Result<()> {
        self.budget.check_operand(what, terms)
    }

    pub(crate) fn reduce(
        &mut self,
        polys: &[Poly],
        variable: usize,
        mut collector: Option<&mut SingCollector>,
    ) -> Result<Vec<Poly>> {
        if polys.is_empty() {
            return Ok(Vec::new());
        }
        self.budget.check_time("Fubini reduction")?;

        if let Some(cached) = self.step_cache.get(variable, polys) {
            if let Some(sings) = collector.as_deref_mut() {
                for factor in cached {
                    sings.observe(factor);
                }
            }
            return Ok(cached.clone());
        }

        let mut candidates =
            Vec::with_capacity(polys.len().saturating_mul(polys.len().saturating_add(3)) / 2);
        for polynomial in polys {
            if polynomial.is_zero() || polynomial.is_rational_constant() {
                continue;
            }
            let degree = polynomial.degree(variable)?;
            if degree < 0 {
                continue;
            }
            let leading = polynomial.coefficient_of(variable, degree)?;
            if !leading.is_zero() {
                candidates.push(leading);
            }
            if degree >= 1 {
                self.budget
                    .check_operand("discriminant input", polynomial.n_terms())?;
                let discriminant = polynomial.discriminant(variable)?;
                if !discriminant.is_zero() {
                    candidates.push(discriminant);
                }
                let constant = polynomial.coefficient_of(variable, 0)?;
                if !constant.is_zero() && !constant.is_rational_constant() {
                    candidates.push(constant);
                }
            }
        }

        for left_index in 0..polys.len() {
            let left = &polys[left_index];
            if left.is_zero() || left.degree(variable)? < 1 {
                continue;
            }
            for right in &polys[left_index + 1..] {
                if right.is_zero() || right.degree(variable)? < 1 {
                    continue;
                }
                self.budget
                    .check_pair("resultant inputs", left.n_terms(), right.n_terms())?;
                let resultant = left.resultant(right, variable)?;
                if !resultant.is_zero() {
                    candidates.push(resultant);
                }
            }
        }

        let mut bases = Vec::new();
        for candidate in candidates {
            if candidate.is_zero() || candidate.is_rational_constant() {
                continue;
            }
            self.budget
                .check_operand("factorization input", candidate.n_terms())?;
            if let Some(cached) = self.factor_cache.get(&candidate) {
                bases.extend(cached.iter().cloned());
                continue;
            }
            let factored = candidate
                .factor()
                .factors
                .into_iter()
                .map(|(base, _)| base)
                .collect::<Vec<_>>();
            bases.extend(factored.iter().cloned());
            self.factor_cache.insert(candidate, factored);
        }
        let output = dedup_proportional(&bases);
        if let Some(sings) = collector {
            for factor in &output {
                sings.observe(factor);
            }
        }
        self.step_cache.insert(variable, polys, output.clone());
        Ok(output)
    }
}

/// One Fubini-reduction step with an isolated memo lifetime.
pub fn st_fubini_lr(polys: &[Poly], variable: usize) -> Result<Vec<Poly>> {
    ReductionEngine::new()?.reduce(polys, variable, None)
}

/// Drop zero/constants and deduplicate by rational proportionality.
pub fn dedup_proportional(polys: &[Poly]) -> Vec<Poly> {
    let mut seen = PolyIndex::default();
    let mut output = Vec::new();
    for polynomial in polys {
        if polynomial.is_zero() || polynomial.is_rational_constant() {
            continue;
        }
        let canonical = polynomial.canonical_proportional_form();
        seen.insert(&mut output, canonical);
    }
    output
}

/// N-way proportional intersection, retaining representatives from the first
/// list in first-encounter order.
pub fn intersect_proportional(lists: &[Vec<Poly>]) -> Vec<Poly> {
    let Some(first) = lists.first() else {
        return Vec::new();
    };
    if lists.len() == 1 {
        return dedup_proportional(first);
    }

    let mut common = dedup_proportional(first);
    for list in &lists[1..] {
        let keys = list
            .iter()
            .filter(|p| !p.is_zero() && !p.is_rational_constant())
            .map(Poly::canonical_proportional_form)
            .collect::<HashSet<_>>();
        common.retain(|polynomial| keys.contains(polynomial));
        if common.is_empty() {
            return Vec::new();
        }
    }
    common
}

/// Proxy for Mathematica's `LeafCount`, used only to rank orders.
pub fn leaf_count_proxy(polys: &[Poly]) -> i64 {
    let mut count = 0_i64;
    for polynomial in polys {
        for term in 0..polynomial.inner().nterms() {
            if !Q.is_one(&polynomial.inner().coefficients[term]) {
                count += 1;
            }
            for &exponent in polynomial.inner().exponents(term) {
                if exponent >= 1 {
                    count += 1;
                }
                if exponent >= 2 {
                    count += 1;
                }
            }
        }
    }
    count
}

/// Shared degree and future-variable admissibility predicate.
pub fn lr_letter_admissible(
    polynomial: &Poly,
    pivot: usize,
    forbidden_after: &[usize],
    max_degree: i64,
) -> Result<bool> {
    let degree = polynomial.degree(pivot)?;
    if degree < 1 || degree > max_degree {
        return Ok(false);
    }
    if degree >= 2 {
        let used = polynomial.used_variable_indices();
        if forbidden_after
            .iter()
            .any(|variable| *variable != pivot && used.contains(variable))
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn subsets_of_size(n: usize, size: usize) -> Vec<u64> {
    if size > n {
        return Vec::new();
    }
    if size == 0 {
        return vec![0];
    }
    let mut output = Vec::new();
    let mut mask = (1_u64 << size) - 1;
    let limit = 1_u64 << n;
    while mask < limit {
        output.push(mask);
        let low = mask & mask.wrapping_neg();
        let next = mask + low;
        mask = (((next ^ mask) >> 2) / low) | next;
    }
    output
}

fn validate_inputs(groups: &[Vec<Poly>], variables: &[usize]) -> Result<()> {
    if variables.len() > 63 {
        return Err(Error::InvalidInput(
            "find_lr_orders supports at most 63 integration variables".into(),
        ));
    }
    let mut unique = HashSet::new();
    if !variables.iter().all(|variable| unique.insert(*variable)) {
        return Err(Error::InvalidInput(
            "integration variable indices must be unique".into(),
        ));
    }
    let Some(reference) = groups.iter().flatten().next() else {
        return Ok(());
    };
    for &variable in variables {
        if variable >= reference.ctx().len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
    }
    for polynomial in groups.iter().flatten() {
        if polynomial.ctx().vars() != reference.ctx().vars() {
            return Err(Error::ContextMismatch);
        }
    }
    Ok(())
}

fn configured_max_degree(algebraic: bool) -> Result<i64> {
    if !algebraic {
        return Ok(1);
    }
    match std::env::var("HF_LR_MAX_DEG") {
        Ok(raw) if !raw.is_empty() => {
            let degree = raw.parse::<i64>().map_err(|_| {
                Error::InvalidInput(format!("HF_LR_MAX_DEG must be an integer, got `{raw}`"))
            })?;
            if degree < 1 {
                return Err(Error::InvalidInput(
                    "HF_LR_MAX_DEG must be at least one".into(),
                ));
            }
            Ok(degree)
        }
        _ => Ok(2),
    }
}

/// Search all integration orders using the intersection-refined subset DP.
pub fn find_lr_orders(
    group_polys: &[Vec<Poly>],
    xvar_indices: &[usize],
    options: LrSearchOptions,
) -> Result<LrResult> {
    find_lr_orders_collect(group_polys, xvar_indices, options, None)
}

/// Search variant that also emits order-independent kinematic divisors.
pub fn find_lr_orders_collect(
    group_polys: &[Vec<Poly>],
    xvar_indices: &[usize],
    options: LrSearchOptions,
    mut collector: Option<&mut SingCollector>,
) -> Result<LrResult> {
    validate_inputs(group_polys, xvar_indices)?;
    if options.score_prune_factor.is_nan() || options.score_prune_factor <= 0.0 {
        return Err(Error::InvalidInput(
            "score_prune_factor must be positive".into(),
        ));
    }
    if let Some(sings) = collector.as_deref_mut() {
        sings.seed(xvar_indices);
    }
    if group_polys.is_empty() || xvar_indices.is_empty() {
        return Ok(LrResult::empty(0.0));
    }

    let group_count = group_polys.len();
    let variable_count = xvar_indices.len();
    let do_carry = options.allow_algebraic_letters && options.carry_discharge;
    let max_degree = configured_max_degree(options.allow_algebraic_letters)?;
    let mut engine = ReductionEngine::new()?;
    let mut set_table = HashMap::<u64, Vec<Vec<Poly>>>::new();
    let mut orders = HashMap::<u64, LrResult>::new();
    let mut chi_caches = options
        .euler_filter
        .then(|| vec![ChiFilterCache::default(); group_count]);
    if options.euler_filter {
        reset_chi_filter_stats();
    }
    set_table.insert(0, group_polys.to_vec());
    orders.insert(0, LrResult::empty(0.0));

    let prune = options.score_prune_factor.is_finite() && !do_carry;
    let mut score_pruned = HashSet::<u64>::new();

    for size in 1..=variable_count {
        let subsets = subsets_of_size(variable_count, size);
        let mut any_live = false;
        for &bits in &subsets {
            engine.check_time("find_lr_orders subset loop")?;
            if prune {
                let any_parent = (0..variable_count).any(|bit| {
                    bits & (1_u64 << bit) != 0 && !score_pruned.contains(&(bits ^ (1_u64 << bit)))
                });
                if !any_parent {
                    score_pruned.insert(bits);
                    continue;
                }
            }

            let mut reduced_groups = vec![Vec::new(); group_count];
            for group in 0..group_count {
                let mut paths = Vec::with_capacity(size);
                for (bit, &pivot) in xvar_indices.iter().enumerate() {
                    if bits & (1_u64 << bit) == 0 {
                        continue;
                    }
                    let parent = bits ^ (1_u64 << bit);
                    if prune && score_pruned.contains(&parent) {
                        continue;
                    }
                    let parent_groups = set_table.get(&parent).ok_or_else(|| {
                        Error::InvalidInput("missing LR parent subset state".into())
                    })?;
                    let reduced =
                        engine.reduce(&parent_groups[group], pivot, collector.as_deref_mut())?;
                    paths.push(reduced);
                }
                reduced_groups[group] = intersect_proportional(&paths);
                if let Some(caches) = chi_caches.as_mut()
                    && !reduced_groups[group].is_empty()
                {
                    let subset_variables = (0..variable_count)
                        .filter(|bit| bits & (1_u64 << bit) != 0)
                        .map(|bit| xvar_indices[bit])
                        .collect::<Vec<_>>();
                    reduced_groups[group] = chi_filter_letters(
                        &group_polys[group],
                        &subset_variables,
                        &reduced_groups[group],
                        &mut caches[group],
                        87_178,
                    );
                }
            }
            set_table.insert(bits, reduced_groups);

            let forbidden_after = if options.allow_algebraic_letters {
                (0..variable_count)
                    .filter(|bit| bits & (1_u64 << bit) == 0)
                    .map(|bit| xvar_indices[bit])
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };

            let mut best = LrResult::empty(f64::INFINITY);
            for (bit, &pivot) in xvar_indices.iter().enumerate() {
                if bits & (1_u64 << bit) == 0 {
                    continue;
                }
                let parent_bits = bits ^ (1_u64 << bit);
                let Some(parent_order) = orders.get(&parent_bits) else {
                    continue;
                };
                if parent_order.is_nolr() {
                    continue;
                }
                let parent_groups = set_table
                    .get(&parent_bits)
                    .ok_or_else(|| Error::InvalidInput("missing LR order parent state".into()))?;
                let mut score = parent_order.score;
                let mut roots = Vec::new();
                let mut admissible = true;
                for group in parent_groups {
                    for letter in group {
                        let degree = letter.degree(pivot)?;
                        if degree < 1 {
                            continue;
                        }
                        if !lr_letter_admissible(letter, pivot, &forbidden_after, max_degree)? {
                            admissible = false;
                            break;
                        }
                        if degree >= 2 && options.allow_algebraic_letters {
                            roots.push(letter.clone());
                        }
                    }
                    if !admissible {
                        break;
                    }
                    score += (leaf_count_proxy(group) as f64).powf(1.15);
                }
                if admissible && score < best.score {
                    best = parent_order.clone();
                    best.order.push(pivot);
                    best.score = score;
                    best.root_polys.extend(roots);
                }
            }
            any_live |= !best.is_nolr();
            orders.insert(bits, best);
        }

        if prune && size < variable_count {
            let best_at_size = subsets
                .iter()
                .filter(|bits| !score_pruned.contains(bits))
                .filter_map(|bits| orders.get(bits))
                .filter(|order| !order.is_nolr())
                .map(|order| order.score)
                .fold(f64::INFINITY, f64::min);
            if best_at_size.is_finite() {
                let cutoff = options.score_prune_factor * best_at_size;
                for bits in &subsets {
                    if orders
                        .get(bits)
                        .is_none_or(|order| order.is_nolr() || order.score > cutoff)
                    {
                        score_pruned.insert(*bits);
                    }
                }
            }
        }

        if !do_carry && !any_live {
            return Ok(LrResult::empty(f64::INFINITY));
        }
        if !do_carry {
            for old in subsets_of_size(variable_count, size - 1) {
                set_table.remove(&old);
            }
        }
    }

    if do_carry {
        let mut driver = CarryDriver {
            table: &set_table,
            variables: xvar_indices,
            best: None,
        };
        driver.visit(
            0,
            &mut Vec::new(),
            0.0,
            &mut Vec::new(),
            crate::integrator::lr_scan::PathState::default(),
        )?;
        return Ok(driver
            .best
            .map(|best| LrResult {
                order: best.order,
                score: best.score,
                root_polys: best.roots,
                carried_sqrts: best.state.nsq,
                kin_sqrts: best.state.nkin,
                terminal_quads: best.state.ntq,
                obligation_polys: best.state.minted,
            })
            .unwrap_or_else(|| LrResult::empty(f64::INFINITY)));
    }

    let full = (1_u64 << variable_count) - 1;
    Ok(orders
        .remove(&full)
        .unwrap_or_else(|| LrResult::empty(f64::INFINITY)))
}

#[derive(Clone)]
struct CarryBest {
    order: Vec<usize>,
    score: f64,
    roots: Vec<Poly>,
    state: crate::integrator::lr_scan::PathState,
}

struct CarryDriver<'a> {
    table: &'a HashMap<u64, Vec<Vec<Poly>>>,
    variables: &'a [usize],
    best: Option<CarryBest>,
}

impl CarryDriver<'_> {
    fn key_beats(
        nsq: u64,
        nonexec: bool,
        score: f64,
        other_nsq: u64,
        other_nonexec: bool,
        other_score: f64,
    ) -> bool {
        (nsq, nonexec, score) < (other_nsq, other_nonexec, other_score)
    }

    fn visit(
        &mut self,
        bits: u64,
        order: &mut Vec<usize>,
        score: f64,
        roots: &mut Vec<Poly>,
        state: crate::integrator::lr_scan::PathState,
    ) -> Result<()> {
        if order.len() == self.variables.len() {
            let replace = self.best.as_ref().is_none_or(|best| {
                Self::key_beats(
                    state.nsq,
                    state.nonexec,
                    score,
                    best.state.nsq,
                    best.state.nonexec,
                    best.score,
                )
            });
            if replace {
                self.best = Some(CarryBest {
                    order: order.clone(),
                    score,
                    roots: roots.clone(),
                    state,
                });
            }
            return Ok(());
        }
        if let Some(best) = &self.best
            && !Self::key_beats(
                state.nsq,
                state.nonexec,
                score,
                best.state.nsq,
                best.state.nonexec,
                best.score,
            )
        {
            return Ok(());
        }

        for bit in 0..self.variables.len() {
            if bits & (1_u64 << bit) != 0 {
                continue;
            }
            let pivot = self.variables[bit];
            let pending = (0..self.variables.len())
                .filter(|next| *next != bit && bits & (1_u64 << next) == 0)
                .map(|next| self.variables[next])
                .collect::<Vec<_>>();
            let (letters, extension, step_roots) = {
                let parent = self.table.get(&bits).ok_or_else(|| {
                    Error::InvalidInput("missing carry-discharge subset state".into())
                })?;
                let mut letters = Vec::new();
                let mut extension = score;
                let mut step_roots = Vec::new();
                for group in parent {
                    extension += (leaf_count_proxy(group) as f64).powf(1.15);
                    for letter in group {
                        if letter.degree(pivot)? == 2 {
                            step_roots.push(letter.clone());
                        }
                        letters.push(letter.clone());
                    }
                }
                (letters, extension, step_roots)
            };
            let mut next_state = state.clone();
            if !crate::integrator::lr_scan::step_fr_judge(
                &letters,
                pivot,
                None,
                &pending,
                self.variables,
                &mut next_state,
            )? {
                continue;
            }

            order.push(pivot);
            let root_mark = roots.len();
            roots.extend(step_roots);
            self.visit(bits | (1_u64 << bit), order, extension, roots, next_state)?;
            roots.truncate(root_mark);
            order.pop();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::core::PolyCtx;

    use super::*;

    fn parse(ctx: &Arc<PolyCtx>, expression: &str) -> Poly {
        Poly::parse(ctx.clone(), expression).unwrap()
    }

    #[test]
    fn fubini_step_includes_endpoint_and_pair_resultant_factors() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let input = vec![parse(&ctx, "1+x*y"), parse(&ctx, "x+y")];
        let output = st_fubini_lr(&input, 0).unwrap();
        let keys = output.into_iter().collect::<HashSet<_>>();
        assert!(keys.contains(&parse(&ctx, "y").canonical_proportional_form()));
        assert!(keys.contains(&parse(&ctx, "y-1").canonical_proportional_form()));
        assert!(keys.contains(&parse(&ctx, "y+1").canonical_proportional_form()));
    }

    #[test]
    fn proportional_dedup_is_structural_and_preserves_first_encounter_order() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let input = vec![
            parse(&ctx, "2*x+2*y"),
            parse(&ctx, "-x-y"),
            parse(&ctx, "x-y"),
            parse(&ctx, "3*x-3*y"),
            parse(&ctx, "7"),
        ];
        let expected = vec![
            parse(&ctx, "x+y").canonical_proportional_form(),
            parse(&ctx, "x-y").canonical_proportional_form(),
        ];

        assert_eq!(dedup_proportional(&input), expected);
    }

    #[test]
    fn structural_reduction_caches_do_not_change_deterministic_ordering() {
        let ctx = PolyCtx::new(["x", "y", "z"]).unwrap();
        let input = vec![
            parse(&ctx, "1+x*y"),
            parse(&ctx, "x+y*z"),
            parse(&ctx, "x+z"),
        ];
        let mut engine = ReductionEngine::new().unwrap();
        let cold = engine.reduce(&input, 0, None).unwrap();
        let warm = engine.reduce(&input, 0, None).unwrap();
        let independent = st_fubini_lr(&input, 0).unwrap();

        assert_eq!(warm, cold);
        assert_eq!(independent, cold);
    }

    #[test]
    fn order_search_prefers_the_lower_leaf_count_path() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let groups = vec![vec![parse(&ctx, "1+x*y"), parse(&ctx, "x+y")]];
        let result = find_lr_orders(&groups, &[0, 1], LrSearchOptions::default()).unwrap();
        assert!(!result.is_nolr());
        assert_eq!(result.order.len(), 2);
        assert_eq!(
            result.order.iter().copied().collect::<HashSet<_>>(),
            HashSet::from([0, 1])
        );
    }

    #[test]
    fn degree_and_future_variable_guard_matches_hyperflint() {
        let ctx = PolyCtx::new(["x", "z", "y"]).unwrap();
        let linear = parse(&ctx, "x+y");
        let quadratic = parse(&ctx, "y*x^2+x+1");
        let future = parse(&ctx, "z*x^2+x+1");
        let cubic = parse(&ctx, "x^3+y");
        assert!(lr_letter_admissible(&linear, 0, &[1], 1).unwrap());
        assert!(!lr_letter_admissible(&quadratic, 0, &[1], 1).unwrap());
        assert!(lr_letter_admissible(&quadratic, 0, &[1], 2).unwrap());
        assert!(!lr_letter_admissible(&future, 0, &[1], 2).unwrap());
        assert!(!lr_letter_admissible(&cubic, 0, &[1], 2).unwrap());
    }

    #[test]
    fn euler_filter_on_off_parity_for_massless_box() {
        let ctx = PolyCtx::new(["x1", "x2", "x3", "x4", "s", "t"]).unwrap();
        let group = ["x1+x2+x3+x4+s*x1*x3+t*x2*x4", "x1", "x2", "x3", "x4"]
            .map(|expression| parse(&ctx, expression))
            .to_vec();
        let off = find_lr_orders(
            std::slice::from_ref(&group),
            &[0, 1, 2, 3],
            LrSearchOptions::default(),
        )
        .unwrap();
        let on = find_lr_orders(
            &[group],
            &[0, 1, 2, 3],
            LrSearchOptions {
                euler_filter: true,
                ..LrSearchOptions::default()
            },
        )
        .unwrap();
        assert_eq!(off.order, on.order);
        assert_eq!(off.score, on.score);
        assert!(!off.is_nolr() && !on.is_nolr());
    }
}
