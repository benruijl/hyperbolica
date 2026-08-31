//! Shared Brown/Fubini polynomial-reduction primitives.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::{Duration, Instant};

use symbolica::prelude::*;

use crate::core::Poly;
use crate::error::{Error, Result};

use super::structural_keys::{PolyIndex, PolySliceCache};

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

    pub(crate) fn seed(&mut self, variables: &[usize]) {
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
pub(crate) struct ReductionBudget {
    started: Instant,
    duration: Option<Duration>,
    max_operand_terms: Option<usize>,
}

impl ReductionBudget {
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

/// Per-request reduction engine. Both memo layers are value-preserving and
/// avoid repeatedly factoring the same subset marginals.
pub(crate) struct ReductionEngine {
    step_cache: PolySliceCache<Vec<Poly>>,
    factor_cache: HashMap<Poly, Vec<Poly>>,
    budget: ReductionBudget,
}

impl ReductionEngine {
    pub(crate) fn new() -> Result<Self> {
        Ok(Self {
            step_cache: PolySliceCache::default(),
            factor_cache: HashMap::new(),
            budget: ReductionBudget::from_environment()?,
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
