//! Fixed-order table construction and stage accounting.

use std::collections::BTreeSet;
use std::time::Instant;

use crate::core::Poly;
use crate::error::{Error, Result};

use super::super::lr_reduction::{ReductionEngine, lr_letter_admissible};
use super::object::{intern, make_object};
use super::{FactorTable, FactorTableLimits, PairEntry, SingletonCoeff, SingletonEntry, StageInfo};

fn max_degree(algebraic_letters: bool) -> Result<i64> {
    if !algebraic_letters {
        return Ok(1);
    }
    match std::env::var("HF_LR_MAX_DEG") {
        Ok(raw) if !raw.is_empty() => {
            let parsed = raw.parse::<i64>().map_err(|_| {
                Error::InvalidInput(format!("HF_LR_MAX_DEG must be an integer, got `{raw}`"))
            })?;
            Ok(parsed.max(1))
        }
        _ => Ok(2),
    }
}

/// Build a fixed-order factor-prediction table.
pub fn factor_table(
    group_polys: &[Vec<Poly>],
    order_indices: &[usize],
    algebraic_letters: bool,
    limits: FactorTableLimits,
) -> Result<FactorTable> {
    let mut unique = BTreeSet::new();
    if !order_indices
        .iter()
        .all(|variable| unique.insert(*variable))
    {
        return Err(Error::InvalidInput(
            "factor-table order indices must be unique".into(),
        ));
    }
    if let Some(reference) = group_polys.iter().flatten().next() {
        for &variable in order_indices {
            if variable >= reference.ctx().len() {
                return Err(Error::UnknownVariable(variable.to_string()));
            }
        }
        for polynomial in group_polys.iter().flatten() {
            if !polynomial.ctx().is_compatible_with(reference.ctx()) {
                return Err(Error::ContextMismatch);
            }
        }
    }

    let maximum_degree = max_degree(algebraic_letters)?;
    let mut engine = ReductionEngine::new()?;
    let mut table = FactorTable::default();
    let mut pair_keys = BTreeSet::<(usize, usize, usize)>::new();
    let mut singleton_keys = BTreeSet::<(usize, usize)>::new();
    let mut current = group_polys.to_vec();

    for (stage_index, &variable) in order_indices.iter().enumerate() {
        engine.check_time("factor-table stage")?;
        let started = Instant::now();
        let forbidden_after = &order_indices[stage_index + 1..];
        let mut stage = StageInfo {
            var_idx: variable,
            admissible: Vec::with_capacity(current.len()),
            pool: Vec::new(),
            pair_count: 0,
            singleton_count: 0,
            inadmissible_count: 0,
            build_seconds: 0.0,
        };
        let mut next = vec![Vec::new(); current.len()];
        let mut stage_pool = BTreeSet::new();

        for group_index in 0..current.len() {
            let output = engine.reduce(&current[group_index], variable, None)?;
            let mut group_pool = Vec::new();
            let mut pool_seen = BTreeSet::new();
            for polynomial in output.iter().chain(&current[group_index]) {
                if polynomial.is_zero() || polynomial.is_rational_constant() {
                    continue;
                }
                let id = intern(&mut table, polynomial);
                if pool_seen.insert(id) {
                    group_pool.push(id);
                }
                stage_pool.insert(id);
            }

            let mut admissible = Vec::new();
            let mut linear = Vec::new();
            for polynomial in &current[group_index] {
                if polynomial.is_zero() || polynomial.is_rational_constant() {
                    continue;
                }
                let degree = polynomial.degree(variable)?;
                if degree < 1 {
                    continue;
                }
                if !lr_letter_admissible(
                    polynomial,
                    variable,
                    if algebraic_letters {
                        forbidden_after
                    } else {
                        &[]
                    },
                    maximum_degree,
                )? {
                    stage.inadmissible_count += 1;
                    continue;
                }
                let id = intern(&mut table, polynomial);
                admissible.push(id);
                if table.intern_polys[id].degree(variable)? == 1 {
                    linear.push(id);
                }
            }

            for &id in &admissible {
                if !singleton_keys.insert((variable, id)) {
                    continue;
                }
                table.stats.singletons_total += 1;
                if table.stats.singletons_total > limits.max_singletons {
                    return Err(Error::InvalidInput(format!(
                        "max_singletons exceeded (reached {})",
                        table.stats.singletons_total
                    )));
                }
                let letter = table.intern_polys[id].clone();
                let degree = letter.degree(variable)?;
                let mut coefficients = Vec::with_capacity((degree + 1) as usize);
                for power in (0..=degree).rev() {
                    let coefficient = letter.coefficient_of(variable, power)?;
                    let object = make_object(&mut table, &[coefficient], &[], &group_pool)?;
                    if object.oop {
                        table.stats.oop += 1;
                    }
                    coefficients.push(SingletonCoeff { power, object });
                }
                let discriminant = if degree == 2 {
                    engine.check_operand("factor-table discriminant", letter.n_terms())?;
                    let disc = letter.discriminant(variable)?;
                    let object = make_object(&mut table, &[disc], &[], &group_pool)?;
                    if object.oop {
                        table.stats.oop += 1;
                    }
                    Some(object)
                } else {
                    None
                };
                table.singletons.push(SingletonEntry {
                    var_idx: variable,
                    id,
                    degree,
                    coefficients,
                    discriminant,
                });
                stage.singleton_count += 1;
            }
            stage.admissible.push(admissible);

            for left_index in 0..linear.len() {
                for right_index in left_index + 1..linear.len() {
                    let mut left = linear[left_index];
                    let mut right = linear[right_index];
                    if left == right {
                        continue;
                    }
                    // Keep the internal pair key independent of encounter
                    // direction without formatting either polynomial. The
                    // JSON bridge restores HyperFLINT's historical lexical
                    // f/g orientation at the transport boundary.
                    if right < left {
                        std::mem::swap(&mut left, &mut right);
                    }
                    if !pair_keys.insert((variable, left, right)) {
                        continue;
                    }
                    table.stats.pairs_total += 1;
                    if table.stats.pairs_total > limits.max_pairs {
                        return Err(Error::InvalidInput(format!(
                            "max_pairs exceeded (reached {})",
                            table.stats.pairs_total
                        )));
                    }
                    let f = table.intern_polys[left].clone();
                    let g = table.intern_polys[right].clone();
                    let lc_f = f.coefficient_of(variable, 1)?;
                    let lc_g = g.coefficient_of(variable, 1)?;
                    let numerator = &(&lc_g * &f) - &(&lc_f * &g);
                    let difference =
                        make_object(&mut table, &[numerator], &[lc_f, lc_g], &group_pool)?;
                    if difference.oop {
                        table.stats.oop += 1;
                        table.stats.pair_fallbacks += 1;
                    }
                    table.pairs.push(PairEntry {
                        var_idx: variable,
                        f_id: left,
                        g_id: right,
                        difference,
                    });
                    stage.pair_count += 1;
                }
            }
            next[group_index] = output;
        }

        stage.pool.extend(stage_pool);
        stage.build_seconds = started.elapsed().as_secs_f64();
        table.stages.push(stage);
        current = next;
    }

    table
        .pairs
        .sort_by_key(|entry| (entry.var_idx, entry.f_id, entry.g_id));
    table
        .singletons
        .sort_by_key(|entry| (entry.var_idx, entry.id));
    Ok(table)
}

/// Naming-compatible alias for callers that prefer the upstream `build` API.
pub fn build(
    group_polys: &[Vec<Poly>],
    order_indices: &[usize],
    algebraic_letters: bool,
    limits: FactorTableLimits,
) -> Result<FactorTable> {
    factor_table(group_polys, order_indices, algebraic_letters, limits)
}
