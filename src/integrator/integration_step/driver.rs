use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use rayon::prelude::*;

use super::contour::{close_positive_letters, regulator_bin_is_zero};
use super::entry::{BinKey, EntryContribution, process_entry};
use super::{
    Boundary, IntegrationError, IntegrationResult, IntegrationStepOptions, ShuffleEntrySym,
    ShuffleList, ShuffleListSym,
};
use crate::core::{PolyCtx, SymCoef};
use crate::error::Error;
use crate::integrator::{
    RegulatorSym, TransformResult, canonicalize_regulator_sym, transform_shuffle,
};
use crate::reduce::MzvReductionTable;

fn merge_contributions(
    ctx: &Arc<PolyCtx>,
    contributions: Vec<EntryContribution>,
    variable: usize,
    table: &MzvReductionTable,
    check_divergences: bool,
    remaining_variables: &[usize],
) -> IntegrationResult<RegulatorSym> {
    let mut finite = RegulatorSym::new();
    let mut zero_bins = BTreeMap::<BinKey, RegulatorSym>::new();
    let mut infinity_bins = BTreeMap::<BinKey, RegulatorSym>::new();
    for contribution in contributions {
        finite.extend(contribution.finite);
        for (key, terms) in contribution.zero_bins {
            zero_bins.entry(key).or_default().extend(terms);
        }
        for (key, terms) in contribution.infinity_bins {
            infinity_bins.entry(key).or_default().extend(terms);
        }
    }

    if check_divergences {
        for (boundary, bins) in [
            (Boundary::Zero, zero_bins),
            (Boundary::Infinity, infinity_bins),
        ] {
            for ((log_power, power), terms) in bins {
                let closed = close_positive_letters(ctx, &terms, variable, table)?;
                if !regulator_bin_is_zero(ctx, &closed, remaining_variables, table)? {
                    return Err(IntegrationError::Divergent {
                        boundary,
                        variable: ctx.vars()[variable].clone(),
                        log_power,
                        power,
                    });
                }
            }
        }
    }
    Ok(canonicalize_regulator_sym(&finite)?)
}

/// Integrate one variable from zero to infinity.
pub fn integration_step(
    ctx: &Arc<PolyCtx>,
    input: &ShuffleList,
    variable: usize,
    table: &MzvReductionTable,
    check_divergences: bool,
) -> IntegrationResult<RegulatorSym> {
    let promoted = input
        .iter()
        .map(|entry| ShuffleEntrySym {
            coef: SymCoef::from_rat(&entry.coef),
            shuffle: entry.shuffle.clone(),
            factored_den: entry.factored_den.clone(),
        })
        .collect::<Vec<_>>();
    integration_step_core_sym_with_options(
        ctx,
        &promoted,
        variable,
        table,
        &IntegrationStepOptions {
            check_divergences,
            ..IntegrationStepOptions::default()
        },
        &[],
    )
}

pub fn integration_step_with_options(
    ctx: &Arc<PolyCtx>,
    input: &ShuffleList,
    variable: usize,
    table: &MzvReductionTable,
    options: &IntegrationStepOptions,
) -> IntegrationResult<RegulatorSym> {
    let promoted = input
        .iter()
        .map(|entry| ShuffleEntrySym {
            coef: SymCoef::from_rat(&entry.coef),
            shuffle: entry.shuffle.clone(),
            factored_den: entry.factored_den.clone(),
        })
        .collect::<Vec<_>>();
    integration_step_core_sym_with_options(ctx, &promoted, variable, table, options, &[])
}

pub fn integration_step_sym(
    ctx: &Arc<PolyCtx>,
    input: &ShuffleListSym,
    variable: usize,
    table: &MzvReductionTable,
    check_divergences: bool,
) -> IntegrationResult<RegulatorSym> {
    integration_step_sym_with_options(
        ctx,
        input,
        variable,
        table,
        &IntegrationStepOptions {
            check_divergences,
            ..IntegrationStepOptions::default()
        },
    )
}

pub fn integration_step_sym_with_options(
    ctx: &Arc<PolyCtx>,
    input: &ShuffleListSym,
    variable: usize,
    table: &MzvReductionTable,
    options: &IntegrationStepOptions,
) -> IntegrationResult<RegulatorSym> {
    let base = integration_step_core_sym_with_options(ctx, input, variable, table, options, &[])?;
    close_positive_letters(ctx, &base, variable, table)
}

/// Unclosed step used between integrations in the multi-variable driver.
///
/// HyperFLINT defers positive-axis contour continuation until the final
/// variable. Keeping this distinct from the public `integration_step_sym`
/// wrapper prevents symbolic residues from being introduced prematurely.
pub(crate) fn integration_step_core_sym_with_options(
    ctx: &Arc<PolyCtx>,
    input: &ShuffleListSym,
    variable: usize,
    table: &MzvReductionTable,
    options: &IntegrationStepOptions,
    remaining_variables: &[usize],
) -> IntegrationResult<RegulatorSym> {
    if variable >= ctx.len() {
        return Err(Error::UnknownVariable(variable.to_string()).into());
    }
    // Transform only the shuffle spine, so equal spines across independently
    // weighted entries share one result. The cache is step-local: it cannot
    // retain context-owned Symbolica objects beyond their useful lifetime.
    let mut transform_cache = HashMap::<String, Arc<TransformResult>>::new();
    let mut transformed = Vec::with_capacity(input.len());
    for entry in input {
        let mut key = String::new();
        for word in &entry.shuffle {
            key.push_str(&word.content_key());
            key.push('\u{2}');
        }
        let value = if let Some(value) = transform_cache.get(&key) {
            value.clone()
        } else {
            let value = Arc::new(transform_shuffle(ctx, &entry.shuffle, variable)?);
            transform_cache.insert(key, value.clone());
            value
        };
        transformed.push(value);
    }

    // Pair indices are allocated in encounter order by the upstream-compatible
    // registry. Keep that encounter order deterministic until the table is
    // replaced by an owned batch allocator; the default rational path retains
    // full Rayon parallelism.
    let contributions =
        if options.parallel && !options.introduce_algebraic_letters && input.len() >= 4 {
            (0..input.len())
                .into_par_iter()
                .map(|index| {
                    process_entry(
                        ctx,
                        &input[index],
                        variable,
                        options.check_divergences,
                        options.introduce_algebraic_letters,
                        remaining_variables,
                        transformed[index].as_ref(),
                    )
                })
                .collect::<IntegrationResult<Vec<_>>>()?
        } else {
            input
                .iter()
                .zip(&transformed)
                .map(|(entry, transformed)| {
                    process_entry(
                        ctx,
                        entry,
                        variable,
                        options.check_divergences,
                        options.introduce_algebraic_letters,
                        remaining_variables,
                        transformed.as_ref(),
                    )
                })
                .collect::<IntegrationResult<Vec<_>>>()?
        };
    merge_contributions(
        ctx,
        contributions,
        variable,
        table,
        options.check_divergences,
        remaining_variables,
    )
}
