//! Projective Cheng--Wu gauge scan and HyperFLINT's strict/FindRoots
//! keep rules.

use std::collections::{HashMap, HashSet};

use symbolica::prelude::*;

use crate::algebra::euler::{ChiFilterCache, chi_filter_letters, reset_chi_filter_stats};
use crate::core::Poly;
use crate::error::{Error, Result};

use super::lr_search::{ReductionEngine, intersect_proportional, leaf_count_proxy};
use super::structural_keys::PolyIndex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeepRule {
    Strict,
    FindRoots,
}

/// Exponent `a + b epsilon` of one input factor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScanExponent {
    pub a: i64,
    pub b: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct ScanOptions {
    pub keep_rule: KeepRule,
    /// Apply the pure-Symbolica Euler-characteristic genuineness filter.
    pub euler_filter: bool,
    pub max_orders: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            keep_rule: KeepRule::Strict,
            euler_filter: false,
            max_orders: 8192,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScanOrder {
    pub order: Vec<usize>,
    pub gauge: usize,
    pub score: f64,
    pub carried_sqrts: u64,
    pub kin_sqrts: u64,
    pub terminal_quads: u64,
}

#[derive(Clone, Debug, Default)]
pub struct ScanResult {
    pub projective: bool,
    pub truncated: bool,
    pub orders: Vec<ScanOrder>,
}

#[derive(Clone, Debug, Default)]
pub struct FrJudgment {
    pub ok: bool,
    pub carry: Vec<Poly>,
    pub kin: u64,
    pub terminal: u64,
}

/// Path-local deferred square-root state.  Sibling DFS branches never share
/// this value.
#[derive(Clone, Debug, Default)]
pub struct PathState {
    pub carried: Vec<Poly>,
    pub nsq: u64,
    pub nkin: u64,
    pub ntq: u64,
    pub nonexec: bool,
    /// Path-wide, proportionality-deduplicated obligation ledger.
    pub minted: Vec<Poly>,
    minted_index: PolyIndex,
}

fn homogeneous_degree(polynomial: &Poly, variables: &[usize]) -> Option<i64> {
    if polynomial.is_zero() {
        return None;
    }
    let mut degree = None;
    for exponents in polynomial.inner().exponents_iter() {
        let term_degree = variables
            .iter()
            .map(|variable| i64::from(exponents[*variable]))
            .sum::<i64>();
        if degree.is_some_and(|first| first != term_degree) {
            return None;
        }
        degree = Some(term_degree);
    }
    degree
}

fn integer_is_square(value: &Integer) -> bool {
    if value.is_negative() {
        return false;
    }
    if value == &0 || value == &1 {
        return true;
    }
    value
        .factor()
        .into_iter()
        .all(|(_, exponent)| (&exponent % 2_i64).is_zero())
}

fn rational_is_square(value: &Rational) -> bool {
    integer_is_square(value.numerator_ref()) && integer_is_square(value.denominator_ref())
}

fn perfect_square(polynomial: &Poly) -> bool {
    if polynomial.is_zero() {
        return true;
    }
    polynomial
        .inner()
        .factor()
        .into_iter()
        .all(|(factor, exponent)| {
            if exponent % 2 == 0 {
                true
            } else if factor.is_constant() {
                rational_is_square(&factor.get_constant())
            } else {
                false
            }
        })
}

fn depends_on_any(polynomial: &Poly, variables: &[usize]) -> Result<bool> {
    for &variable in variables {
        if polynomial.degree(variable)? > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

fn degauge(polynomial: &Poly, gauge: Option<usize>) -> Result<Poly> {
    match gauge {
        Some(variable) if polynomial.degree(variable)? > 0 => {
            polynomial.substitute_rational(variable, &Rational::one())
        }
        _ => Ok(polynomial.clone()),
    }
}

/// Detect projectivity group by group: all factors must be homogeneous in
/// the integration variables and both epsilon-weighted degree identities
/// must hold.
pub fn projective_input(
    group_polys: &[Vec<Poly>],
    xvar_indices: &[usize],
    exponents: &[Vec<ScanExponent>],
) -> Result<bool> {
    if group_polys.is_empty() || exponents.len() != group_polys.len() {
        return Ok(false);
    }
    let n = i64::try_from(xvar_indices.len())
        .map_err(|_| Error::InvalidInput("too many integration variables".into()))?;
    let reference_variables = group_polys
        .iter()
        .flatten()
        .next()
        .map(|polynomial| polynomial.ctx().vars());
    for (group, powers) in group_polys.iter().zip(exponents) {
        if group.len() != powers.len() {
            return Ok(false);
        }
        let mut sum_a = 0_i64;
        let mut sum_b = 0_i64;
        for (polynomial, exponent) in group.iter().zip(powers) {
            if reference_variables.is_some_and(|vars| polynomial.ctx().vars() != vars) {
                return Err(Error::ContextMismatch);
            }
            for &variable in xvar_indices {
                if variable >= polynomial.ctx().len() {
                    return Err(Error::UnknownVariable(variable.to_string()));
                }
            }
            let Some(degree) = homogeneous_degree(polynomial, xvar_indices) else {
                return Ok(false);
            };
            sum_a =
                sum_a
                    .checked_add(exponent.a.checked_mul(degree).ok_or_else(|| {
                        Error::InvalidInput("projectivity degree overflow".into())
                    })?)
                    .ok_or_else(|| Error::InvalidInput("projectivity degree overflow".into()))?;
            sum_b =
                sum_b
                    .checked_add(exponent.b.checked_mul(degree).ok_or_else(|| {
                        Error::InvalidInput("projectivity degree overflow".into())
                    })?)
                    .ok_or_else(|| Error::InvalidInput("projectivity degree overflow".into()))?;
        }
        if sum_a != -n || sum_b != 0 {
            return Ok(false);
        }
    }
    Ok(true)
}

/// A quadratic is Euler-conic rationalizable iff its leading and constant
/// coefficients in the pivot are rational polynomial squares.
pub fn conic_rationalizable(letter: &Poly, pivot: usize) -> Result<bool> {
    if letter.degree(pivot)? != 2 {
        return Ok(false);
    }
    Ok(perfect_square(&letter.coefficient_of(pivot, 2)?)
        && perfect_square(&letter.coefficient_of(pivot, 0)?))
}

/// FindRoots judgment for one letter.
pub fn fr_judge(
    letter: &Poly,
    pivot: usize,
    pending: &[usize],
    _all_xvars: &[usize],
) -> Result<FrJudgment> {
    let degree = letter.degree(pivot)?;
    if degree <= 1 {
        return Ok(FrJudgment {
            ok: true,
            ..FrJudgment::default()
        });
    }
    if degree >= 3 {
        return Ok(FrJudgment::default());
    }
    if conic_rationalizable(letter, pivot)? {
        return Ok(FrJudgment {
            ok: true,
            ..FrJudgment::default()
        });
    }
    if pending.is_empty() {
        return Ok(FrJudgment {
            ok: true,
            terminal: 1,
            ..FrJudgment::default()
        });
    }

    let discriminant = letter.discriminant(pivot)?;
    if discriminant.is_zero() {
        return Ok(FrJudgment {
            ok: true,
            ..FrJudgment::default()
        });
    }

    let mut judgment = FrJudgment {
        ok: true,
        ..FrJudgment::default()
    };
    let mut seen = PolyIndex::default();
    for (factor, exponent) in discriminant.factor().factors {
        if exponent % 2 == 0 || factor.is_rational_constant() {
            continue;
        }
        let canonical = factor.canonical_proportional_form();
        if depends_on_any(&canonical, pending)? {
            seen.insert(&mut judgment.carry, canonical);
        } else {
            judgment.kin = 1;
        }
    }
    Ok(judgment)
}

/// One FindRoots carry/discharge step, shared by the gauge scan and the
/// gauge-free LR order search.
pub fn step_fr_judge(
    letters: &[Poly],
    pivot: usize,
    gauge: Option<usize>,
    pending: &[usize],
    all_integration_variables: &[usize],
    state: &mut PathState,
) -> Result<bool> {
    let mut carried = Vec::new();
    let mut carried_index = PolyIndex::default();
    for obligation in &state.carried {
        if depends_on_any(obligation, pending)? || obligation.degree(pivot)? > 0 {
            let canonical = obligation.canonical_proportional_form();
            carried_index.insert(&mut carried, canonical);
        }
    }

    let mut judged = Vec::new();
    let mut judged_index = PolyIndex::default();
    for letter in letters {
        let polynomial = degauge(letter, gauge)?;
        if polynomial.is_rational_constant() {
            continue;
        }
        let canonical = polynomial.canonical_proportional_form();
        judged_index.insert(&mut judged, canonical);
    }
    for obligation in &carried {
        let canonical = obligation.canonical_proportional_form();
        judged_index.insert(&mut judged, canonical);
    }

    for polynomial in judged {
        let judgment = fr_judge(&polynomial, pivot, pending, pending)?;
        if !judgment.ok {
            return Ok(false);
        }
        state.nkin += judgment.kin;
        state.ntq += judgment.terminal;
        for obligation in judgment.carry {
            let canonical = obligation.canonical_proportional_form();
            let (_, inserted) = carried_index.insert(&mut carried, canonical.clone());
            if inserted {
                state.nsq += 1;
                let mut support = 0_usize;
                let mut maximum_degree = 0_i64;
                for &variable in all_integration_variables {
                    let degree = canonical.degree(variable)?;
                    if degree > 0 {
                        support += 1;
                        maximum_degree = maximum_degree.max(degree);
                    }
                }
                if support > 1 || maximum_degree > 2 {
                    state.nonexec = true;
                }
                state.minted_index.insert(&mut state.minted, canonical);
            }
        }
    }
    state.carried = carried;
    Ok(true)
}

struct ScanDriver<'a> {
    table: &'a HashMap<u64, Vec<Vec<Poly>>>,
    variables: &'a [usize],
    rule: KeepRule,
    max_orders: usize,
    result: ScanResult,
}

impl ScanDriver<'_> {
    fn visit(
        &mut self,
        bits: u64,
        order: &mut Vec<usize>,
        score: f64,
        gauge_position: usize,
        state: PathState,
    ) -> Result<()> {
        if self.result.truncated {
            return Ok(());
        }
        if order.len() == self.variables.len() - 1 {
            if self.result.orders.len() >= self.max_orders {
                self.result.truncated = true;
                return Ok(());
            }
            self.result.orders.push(ScanOrder {
                order: order.clone(),
                gauge: self.variables[gauge_position],
                score,
                carried_sqrts: state.nsq,
                kin_sqrts: state.nkin,
                terminal_quads: state.ntq,
            });
            return Ok(());
        }

        for bit in 0..self.variables.len() {
            if bit == gauge_position || bits & (1_u64 << bit) != 0 {
                continue;
            }
            let (letters, next_score) = {
                let Some(parent) = self.table.get(&bits) else {
                    return Ok(());
                };
                let letters = parent.iter().flatten().cloned().collect::<Vec<_>>();
                let next_score = parent.iter().fold(score, |accumulator, group| {
                    accumulator + (leaf_count_proxy(group) as f64).powf(1.15)
                });
                (letters, next_score)
            };
            let pivot = self.variables[bit];
            let gauge = self.variables[gauge_position];
            let pending = (0..self.variables.len())
                .filter(|position| {
                    *position != gauge_position
                        && *position != bit
                        && bits & (1_u64 << position) == 0
                })
                .map(|position| self.variables[position])
                .collect::<Vec<_>>();

            let mut next_state = state.clone();
            let admissible = match self.rule {
                KeepRule::Strict => {
                    let mut ok = true;
                    for letter in &letters {
                        let polynomial = degauge(letter, Some(gauge))?;
                        if polynomial.is_rational_constant() {
                            continue;
                        }
                        let degree = polynomial.degree(pivot)?;
                        if degree <= 1 || (degree == 2 && conic_rationalizable(&polynomial, pivot)?)
                        {
                            continue;
                        }
                        ok = false;
                        break;
                    }
                    ok
                }
                KeepRule::FindRoots => step_fr_judge(
                    &letters,
                    pivot,
                    Some(gauge),
                    &pending,
                    self.variables,
                    &mut next_state,
                )?,
            };
            if !admissible {
                continue;
            }
            order.push(pivot);
            self.visit(
                bits | (1_u64 << bit),
                order,
                next_score,
                gauge_position,
                next_state,
            )?;
            order.pop();
        }
        Ok(())
    }
}

fn subsets_of_size(n: usize, size: usize) -> Vec<u64> {
    if size == 0 {
        return vec![0];
    }
    let mut result = Vec::new();
    let mut mask = (1_u64 << size) - 1;
    let limit = 1_u64 << n;
    while mask < limit {
        result.push(mask);
        let low = mask & mask.wrapping_neg();
        let next = mask + low;
        mask = (((next ^ mask) >> 2) / low) | next;
    }
    result
}

/// Exhaustively scan all Cheng--Wu gauges and admissible orders.
pub fn find_lr_orders_scan(
    group_polys: &[Vec<Poly>],
    xvar_indices: &[usize],
    exponents: &[Vec<ScanExponent>],
    options: ScanOptions,
) -> Result<ScanResult> {
    let projective = projective_input(group_polys, xvar_indices, exponents)?;
    let initial = ScanResult {
        projective,
        ..ScanResult::default()
    };
    if !projective {
        return Ok(initial);
    }
    if !(2..=62).contains(&xvar_indices.len()) {
        return Ok(initial);
    }
    let context = group_polys
        .iter()
        .flatten()
        .next()
        .ok_or_else(|| Error::InvalidInput("projective scan has no polynomials".into()))?
        .ctx()
        .clone();
    let mut seen_variables = HashSet::new();
    for &variable in xvar_indices {
        if variable >= context.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if !seen_variables.insert(variable) {
            return Err(Error::InvalidInput(
                "scan integration variable indices must be unique".into(),
            ));
        }
    }

    let mut augmented = group_polys.to_vec();
    for group in &mut augmented {
        for &variable in xvar_indices {
            group.push(Poly::generator(context.clone(), variable)?);
        }
    }
    let mut engine = ReductionEngine::new()?;
    let mut table = HashMap::<u64, Vec<Vec<Poly>>>::new();
    let mut chi_caches = options
        .euler_filter
        .then(|| vec![ChiFilterCache::default(); augmented.len()]);
    if options.euler_filter {
        reset_chi_filter_stats();
    }
    table.insert(0, augmented.clone());
    for size in 1..xvar_indices.len() {
        for bits in subsets_of_size(xvar_indices.len(), size) {
            engine.check_time("find_lr_orders_scan subset loop")?;
            let mut entry = vec![Vec::new(); augmented.len()];
            for group in 0..augmented.len() {
                let mut paths = Vec::with_capacity(size);
                for (bit, &pivot) in xvar_indices.iter().enumerate() {
                    if bits & (1_u64 << bit) == 0 {
                        continue;
                    }
                    let parent = bits ^ (1_u64 << bit);
                    let Some(parent_groups) = table.get(&parent) else {
                        continue;
                    };
                    paths.push(engine.reduce(&parent_groups[group], pivot, None)?);
                }
                entry[group] = intersect_proportional(&paths);
                if let Some(caches) = chi_caches.as_mut()
                    && !entry[group].is_empty()
                {
                    let subset_variables = (0..xvar_indices.len())
                        .filter(|bit| bits & (1_u64 << bit) != 0)
                        .map(|bit| xvar_indices[bit])
                        .collect::<Vec<_>>();
                    entry[group] = chi_filter_letters(
                        &augmented[group],
                        &subset_variables,
                        &entry[group],
                        &mut caches[group],
                        87_178,
                    );
                }
            }
            table.insert(bits, entry);
        }
    }

    let mut driver = ScanDriver {
        table: &table,
        variables: xvar_indices,
        rule: options.keep_rule,
        max_orders: options.max_orders,
        result: initial,
    };
    for gauge_position in 0..xvar_indices.len() {
        driver.visit(
            0,
            &mut Vec::new(),
            0.0,
            gauge_position,
            PathState::default(),
        )?;
        if driver.result.truncated {
            break;
        }
    }
    driver.result.orders.sort_by(|left, right| {
        left.score
            .total_cmp(&right.score)
            .then_with(|| left.gauge.cmp(&right.gauge))
            .then_with(|| left.order.cmp(&right.order))
    });
    Ok(driver.result)
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
    fn projectivity_and_inhomogeneity_match_the_doppio_oracle() {
        let ctx = PolyCtx::new([
            "x1", "x2", "x3", "x4", "x5", "qq1", "qq2", "wb1", "wb2", "yb",
        ])
        .unwrap();
        let group = vec![
            parse(&ctx, "x1+x2+x3"),
            parse(
                &ctx,
                "-qq1*x1*x2-qq2*x1*x3+2*wb1*x3*x4-x4^2+2*wb2*x2*x5-x5^2+2*yb*x4*x5",
            ),
        ];
        assert!(
            projective_input(
                std::slice::from_ref(&group),
                &[0, 1, 2, 3, 4],
                &[vec![
                    ScanExponent { a: 1, b: 2 },
                    ScanExponent { a: -3, b: -1 }
                ]],
            )
            .unwrap()
        );
        assert!(
            !projective_input(
                &[group],
                &[0, 1, 2, 3, 4],
                &[vec![
                    ScanExponent { a: 1, b: 0 },
                    ScanExponent { a: -2, b: 0 }
                ]],
            )
            .unwrap()
        );
    }

    #[test]
    fn conic_square_semantics_reject_irrational_and_negative_units() {
        let ctx = PolyCtx::new(["x", "z", "s", "q"]).unwrap();
        assert!(conic_rationalizable(&parse(&ctx, "z^2*x^2+s*x+1"), 0).unwrap());
        assert!(conic_rationalizable(&parse(&ctx, "4*x^2+s*x+9"), 0).unwrap());
        assert!(!conic_rationalizable(&parse(&ctx, "2*x^2+s*x+1"), 0).unwrap());
        assert!(!conic_rationalizable(&parse(&ctx, "-x^2+s*x+1"), 0).unwrap());
        assert!(!conic_rationalizable(&parse(&ctx, "q*x^2+s*x+1"), 0).unwrap());
    }

    #[test]
    fn find_roots_judgment_handles_zero_mixed_and_terminal_discriminants() {
        let ctx = PolyCtx::new(["x", "z", "s", "t", "y", "g"]).unwrap();
        let zero_disc = fr_judge(&parse(&ctx, "g*x^2-2*g*x*z+g*z^2"), 0, &[1], &[0, 1]).unwrap();
        assert!(zero_disc.ok && zero_disc.carry.is_empty() && zero_disc.kin == 0);

        let mixed = fr_judge(&parse(&ctx, "x^2-s*z^2+t*z^2+s*y-t*y"), 0, &[1], &[0, 1]).unwrap();
        assert!(mixed.ok && mixed.carry.len() == 1 && mixed.kin == 1);

        let terminal = fr_judge(&parse(&ctx, "s*z^2+t*z+1"), 1, &[], &[0, 1]).unwrap();
        assert!(terminal.ok && terminal.terminal == 1);
        assert!(
            !fr_judge(&parse(&ctx, "x^3-s"), 0, &[1], &[0, 1])
                .unwrap()
                .ok
        );
    }

    #[test]
    fn carry_ledgers_deduplicate_structurally_across_letters_and_steps() {
        let ctx = PolyCtx::new(["x", "z", "s"]).unwrap();
        let letters = vec![parse(&ctx, "x^2+x-s*z"), parse(&ctx, "x^2-x-s*z")];
        let mut state = PathState::default();

        assert!(step_fr_judge(&letters, 0, None, &[1], &[0, 1], &mut state).unwrap());
        assert_eq!(state.nsq, 1);
        assert_eq!(state.carried.len(), 1);
        assert_eq!(state.minted.len(), 1);

        assert!(step_fr_judge(&letters, 0, None, &[1], &[0, 1], &mut state).unwrap());
        assert_eq!(state.nsq, 1);
        assert_eq!(state.carried.len(), 1);
        assert_eq!(state.minted.len(), 1);
    }

    #[test]
    fn projective_scan_accepts_native_euler_filter() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let group = vec![parse(&ctx, "x+y")];
        let result = find_lr_orders_scan(
            &[group],
            &[0, 1],
            &[vec![ScanExponent { a: -2, b: 0 }]],
            ScanOptions {
                euler_filter: true,
                ..ScanOptions::default()
            },
        )
        .unwrap();
        assert!(result.projective);
        assert!(!result.orders.is_empty());
    }
}
