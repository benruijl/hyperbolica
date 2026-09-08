use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use rayon::prelude::*;

use super::contour::{close_positive_letters, regulator_bin_is_zero};
use super::entry::{BinKey, EntryContribution, process_entry};
use super::{
    Boundary, IntegrationError, IntegrationResult, IntegrationStepOptions, ShuffleEntrySym,
    ShuffleList, ShuffleListSym,
};
use crate::algebra::algebraic_letters::join_algebraic_letter_session;
use crate::core::{DigestBuckets, PolyCtx, SymCoef, structural_bucket_digest};
use crate::error::Error;
use crate::integrator::transform::TransformSession;
use crate::integrator::{
    RegulatorSym, TransformOptions, TransformResult, canonicalize_regulator_sym,
};
use crate::reduce::MzvReductionTable;
use crate::symbols::Word;

const STEP_TRANSFORM_BUCKET_DOMAIN: u64 = 0x4953_5452_4348_0001;

struct StepTransformCacheEntry {
    variable: usize,
    shuffle: Vec<Word>,
    value: Arc<TransformResult>,
}

#[derive(Default)]
pub(super) struct StepTransformCache {
    buckets: DigestBuckets,
    entries: Vec<StepTransformCacheEntry>,
}

impl StepTransformCache {
    fn digest(shuffle: &[Word], variable: usize) -> u64 {
        structural_bucket_digest(STEP_TRANSFORM_BUCKET_DOMAIN, &(variable, shuffle))
    }

    fn get(&self, shuffle: &[Word], variable: usize) -> Option<&Arc<TransformResult>> {
        self.get_in_bucket(shuffle, variable, Self::digest(shuffle, variable))
    }

    pub(super) fn get_in_bucket(
        &self,
        shuffle: &[Word],
        variable: usize,
        digest: u64,
    ) -> Option<&Arc<TransformResult>> {
        self.buckets
            .find(digest, |index| {
                self.entries.get(index).is_some_and(|entry| {
                    entry.variable == variable && entry.shuffle.as_slice() == shuffle
                })
            })
            .map(|index| &self.entries[index].value)
    }

    fn insert(&mut self, shuffle: &[Word], variable: usize, value: Arc<TransformResult>) {
        self.insert_in_bucket(shuffle, variable, value, Self::digest(shuffle, variable));
    }

    pub(super) fn insert_in_bucket(
        &mut self,
        shuffle: &[Word],
        variable: usize,
        value: Arc<TransformResult>,
        digest: u64,
    ) {
        if let Some(index) = self.buckets.find(digest, |index| {
            self.entries.get(index).is_some_and(|entry| {
                entry.variable == variable && entry.shuffle.as_slice() == shuffle
            })
        }) {
            self.entries[index].value = value;
            return;
        }
        let index = self.entries.len();
        self.entries.push(StepTransformCacheEntry {
            variable,
            shuffle: shuffle.to_vec(),
            value,
        });
        self.buckets.insert(digest, index);
    }
}

fn merge_contributions(
    ctx: &Arc<PolyCtx>,
    contributions: Vec<EntryContribution>,
    variable: usize,
    table: &MzvReductionTable,
    check_divergences: bool,
    fibration_variables: &[usize],
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
                if !regulator_bin_is_zero(ctx, &closed, fibration_variables, table)? {
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
            factored_coefficient: entry.factored_coefficient().cloned(),
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
    integration_step_with_options_and_remaining_variables(ctx, input, variable, table, options, &[])
}

/// Integrate one variable while retaining the variables that survive this
/// step in divergence zero tests.
///
/// The extra variables are projected through the fibration basis before a
/// boundary bin is declared non-zero. This is required for cancellations that
/// depend on later integration variables. They also guard algebraic-letter
/// introduction, so each entry must be a distinct variable scheduled after
/// `variable` and must not repeat `variable` itself.
pub fn integration_step_with_options_and_remaining_variables(
    ctx: &Arc<PolyCtx>,
    input: &ShuffleList,
    variable: usize,
    table: &MzvReductionTable,
    options: &IntegrationStepOptions,
    remaining_variables: &[usize],
) -> IntegrationResult<RegulatorSym> {
    validate_remaining_variables(ctx, variable, remaining_variables)?;
    let promoted = input
        .iter()
        .map(|entry| ShuffleEntrySym {
            coef: SymCoef::from_rat(&entry.coef),
            shuffle: entry.shuffle.clone(),
            factored_coefficient: entry.factored_coefficient().cloned(),
        })
        .collect::<Vec<_>>();
    integration_step_core_sym_with_options(
        ctx,
        &promoted,
        variable,
        table,
        options,
        remaining_variables,
        remaining_variables,
    )
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
    integration_step_sym_with_options_and_remaining_variables(
        ctx,
        input,
        variable,
        table,
        options,
        &[],
    )
}

/// SymCoef-valued variant of
/// [`integration_step_with_options_and_remaining_variables`].
pub fn integration_step_sym_with_options_and_remaining_variables(
    ctx: &Arc<PolyCtx>,
    input: &ShuffleListSym,
    variable: usize,
    table: &MzvReductionTable,
    options: &IntegrationStepOptions,
    remaining_variables: &[usize],
) -> IntegrationResult<RegulatorSym> {
    validate_remaining_variables(ctx, variable, remaining_variables)?;
    let base = integration_step_core_sym_with_options(
        ctx,
        input,
        variable,
        table,
        options,
        remaining_variables,
        remaining_variables,
    )?;
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
    forbidden_algebraic_variables: &[usize],
    divergence_fibration_variables: &[usize],
) -> IntegrationResult<RegulatorSym> {
    if variable >= ctx.len() {
        return Err(Error::UnknownVariable(variable.to_string()).into());
    }
    if let Some(&remaining) = forbidden_algebraic_variables
        .iter()
        .find(|&&remaining| remaining >= ctx.len())
    {
        return Err(Error::UnknownVariable(remaining.to_string()).into());
    }
    if let Some(&fibration) = divergence_fibration_variables
        .iter()
        .find(|&&fibration| fibration >= ctx.len())
    {
        return Err(Error::UnknownVariable(fibration.to_string()).into());
    }
    let _algebraic_session = options
        .introduce_algebraic_letters
        .then(join_algebraic_letter_session)
        .transpose()?;
    // Transform only the shuffle spine, so equal spines across independently
    // weighted entries share one result. The cache is step-local: it cannot
    // retain context-owned Symbolica objects beyond their useful lifetime.
    let mut transform_cache = StepTransformCache::default();
    let mut transformed = Vec::with_capacity(input.len());
    let transform_options = TransformOptions {
        introduce_algebraic_letters: options.introduce_algebraic_letters,
        forbidden_variables: forbidden_algebraic_variables,
    };
    let mut transform_session =
        TransformSession::new(ctx, variable, transform_options, Some(table))?;
    for entry in input {
        let value = if let Some(value) = transform_cache.get(&entry.shuffle, variable) {
            value.clone()
        } else {
            let value = Arc::new(transform_session.transform(&entry.shuffle)?);
            transform_cache.insert(&entry.shuffle, variable, value.clone());
            value
        };
        transformed.push(value);
    }
    // All entries now own their shared results. Release recursive subwords
    // and lookup keys before the memory-intensive primitive/endpoint phase.
    drop(transform_session);
    drop(transform_cache);

    // Pair indices are allocated in encounter order by the upstream-compatible
    // registry. Keep that encounter order deterministic until the table is
    // replaced by an owned batch allocator; the default rational path retains
    // full licensed Rayon parallelism, while restricted mode stays on the
    // calling thread.
    let contributions = if options.parallel
        && !options.introduce_algebraic_letters
        && input.len() >= 4
        && symbolica::LicenseManager::max_threads(rayon::current_num_threads()) > 1
    {
        (0..input.len())
            .into_par_iter()
            .map(|index| {
                process_entry(
                    ctx,
                    &input[index],
                    variable,
                    options.check_divergences,
                    options.introduce_algebraic_letters,
                    forbidden_algebraic_variables,
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
                    forbidden_algebraic_variables,
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
        divergence_fibration_variables,
    )
}

fn validate_remaining_variables(
    ctx: &Arc<PolyCtx>,
    variable: usize,
    remaining_variables: &[usize],
) -> IntegrationResult<()> {
    if variable >= ctx.len() {
        return Err(Error::UnknownVariable(variable.to_string()).into());
    }
    let mut seen = HashSet::with_capacity(remaining_variables.len());
    for &remaining in remaining_variables {
        if remaining >= ctx.len() {
            return Err(Error::UnknownVariable(remaining.to_string()).into());
        }
        if remaining == variable {
            return Err(Error::InvalidInput(format!(
                "remaining variable `{}` is the current integration variable",
                ctx.vars()[remaining]
            ))
            .into());
        }
        if !seen.insert(remaining) {
            return Err(Error::InvalidInput(format!(
                "remaining variable `{}` is listed more than once",
                ctx.vars()[remaining]
            ))
            .into());
        }
    }
    Ok(())
}
