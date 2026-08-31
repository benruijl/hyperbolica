//! Subset reduction-table construction and admissible-order DFS.

use std::collections::HashMap;

use crate::algebra::euler::{ChiFilterCache, chi_filter_letters, reset_chi_filter_stats};
use crate::core::Poly;
use crate::error::Result;
use crate::integrator::lr_find_roots::degauge;
use crate::integrator::lr_reduction::{ReductionEngine, intersect_proportional, leaf_count_proxy};

use super::projective::augment_groups;
use super::{
    KeepRule, PathState, ScanExponent, ScanOptions, ScanOrder, ScanResult, conic_rationalizable,
    projective_input, step_fr_judge,
};

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

fn build_reduction_table(
    augmented: &[Vec<Poly>],
    xvar_indices: &[usize],
    euler_filter: bool,
) -> Result<HashMap<u64, Vec<Vec<Poly>>>> {
    let mut engine = ReductionEngine::new()?;
    let mut table = HashMap::<u64, Vec<Vec<Poly>>>::new();
    let mut chi_caches = euler_filter.then(|| vec![ChiFilterCache::default(); augmented.len()]);
    if euler_filter {
        reset_chi_filter_stats();
    }
    table.insert(0, augmented.to_vec());
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
    Ok(table)
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

    let augmented = augment_groups(group_polys, xvar_indices)?;
    let table = build_reduction_table(&augmented, xvar_indices, options.euler_filter)?;
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
