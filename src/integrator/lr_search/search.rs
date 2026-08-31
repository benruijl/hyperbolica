use std::collections::{HashMap, HashSet};

use crate::algebra::euler::{ChiFilterCache, chi_filter_letters, reset_chi_filter_stats};
use crate::core::Poly;
use crate::error::{Error, Result};

use super::super::lr_reduction::{
    SingCollector, intersect_proportional, leaf_count_proxy, lr_letter_admissible,
};
use super::carry::carry_discharge_result;
use super::input::{configured_max_degree, subsets_of_size, validate_inputs};
use super::{LrResult, LrSearchOptions, ReductionEngine};

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
        return carry_discharge_result(&set_table, xvar_indices);
    }

    let full = (1_u64 << variable_count) - 1;
    Ok(orders
        .remove(&full)
        .unwrap_or_else(|| LrResult::empty(f64::INFINITY)))
}
