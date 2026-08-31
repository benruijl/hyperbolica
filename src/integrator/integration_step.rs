use std::collections::{BTreeMap, HashMap};
use std::fmt::{Display, Formatter};
use std::sync::Arc;

use rayon::prelude::*;

use crate::core::{FactoredRat, Poly, PolyCtx, Rat, SymCoef, SymMonomial};
use crate::error::Error;
use crate::reduce::{
    MzvReductionTable, OnAxisSymEntry, WordlistSym, WordlistSymTerm, break_up_contour_sym,
    test_zero_function_sym,
};
use crate::series::expansions::{expand_infinity_word_in_context, expand_zero_word_in_context};
use crate::series::laurent::{
    coefficient_at_zero, series_expansion, substitute_variable_reciprocal,
};
use crate::symbols::{Word, Wordlist, WordlistTerm};

use super::{
    IntegrateIiOptions, RegKey, RegTermSym, RegulatorSym, canonicalize_regkey,
    canonicalize_regulator_sym, integrate_ii_with_options, regzero_word_in_ctx, transform_shuffle,
};

/// One rational coefficient times a shuffle product of words.
#[derive(Clone, Debug)]
pub struct ShuffleEntry {
    pub coef: Rat,
    pub shuffle: Vec<Word>,
    /// Optional deferred representation for a bare rational integrand.
    /// The current Symbolica partial-fraction layer materializes it once at
    /// the entry boundary; retaining the field keeps the API compatible with
    /// the stay-factored optimization path.
    pub factored_den: Option<FactoredRat>,
}

impl ShuffleEntry {
    pub fn new(coef: Rat, shuffle: Vec<Word>) -> Self {
        Self {
            coef,
            shuffle,
            factored_den: None,
        }
    }
}

pub type ShuffleList = Vec<ShuffleEntry>;

#[derive(Clone, Debug)]
pub struct ShuffleEntrySym {
    pub coef: SymCoef,
    pub shuffle: Vec<Word>,
    pub factored_den: Option<FactoredRat>,
}

impl ShuffleEntrySym {
    pub fn new(coef: SymCoef, shuffle: Vec<Word>) -> Self {
        Self {
            coef,
            shuffle,
            factored_den: None,
        }
    }
}

pub type ShuffleListSym = Vec<ShuffleEntrySym>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boundary {
    Zero,
    Infinity,
}

impl Display for Boundary {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Zero => "zero",
            Self::Infinity => "infinity",
        })
    }
}

/// Failures with semantic meaning at the integration-pipeline boundary.
#[derive(Debug, thiserror::Error)]
pub enum IntegrationError {
    #[error(transparent)]
    Algebra(#[from] Error),

    #[error(
        "HyperFLINT: divergent integral at {boundary} for `{variable}`: \
         Log[{variable}]^{log_power} / {variable}^{power}"
    )]
    Divergent {
        boundary: Boundary,
        variable: String,
        log_power: i64,
        power: i64,
    },
}

pub type IntegrationResult<T> = std::result::Result<T, IntegrationError>;

#[derive(Clone, Debug)]
pub struct IntegrationStepOptions {
    pub check_divergences: bool,
    /// Parallelize four or more independent shuffle entries. Indexed Rayon
    /// collection followed by a serial merge keeps output deterministic.
    pub parallel: bool,
    pub introduce_algebraic_letters: bool,
}

impl Default for IntegrationStepOptions {
    fn default() -> Self {
        Self {
            check_divergences: false,
            parallel: true,
            introduce_algebraic_letters: false,
        }
    }
}

type BinKey = (i64, i64);

#[derive(Default)]
struct EntryContribution {
    finite: RegulatorSym,
    zero_bins: BTreeMap<BinKey, RegulatorSym>,
    infinity_bins: BTreeMap<BinKey, RegulatorSym>,
}

fn scale_wordlist(wordlist: &Wordlist, scalar: &Rat) -> Result<Wordlist, Error> {
    wordlist
        .terms
        .iter()
        .map(|term| {
            Ok(WordlistTerm::new(
                term.coef.try_mul(scalar)?,
                term.word.clone(),
            ))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Wordlist::new)
}

fn pure_symbolic_factor(ctx: &Arc<PolyCtx>, monomial: &SymMonomial) -> SymCoef {
    let mut symbolic = monomial.clone();
    symbolic.prefactor = Rat::one(ctx.clone());
    SymCoef::from_monomials(ctx.clone(), vec![symbolic])
}

fn combine_keys(left: &RegKey, right: &RegKey) -> RegKey {
    let mut combined = Vec::with_capacity(left.len() + right.len());
    combined.extend(left.iter().cloned());
    combined.extend(right.iter().cloned());
    canonicalize_regkey(&combined)
}

fn log_row_as_polynomial(ctx: &Arc<PolyCtx>, row: &[Rat], variable: usize) -> Result<Rat, Error> {
    let generator = Rat::from_poly(Poly::generator(ctx.clone(), variable)?);
    let mut output = Rat::zero(ctx.clone());
    for (power, coefficient) in row.iter().enumerate() {
        if coefficient.is_zero() {
            continue;
        }
        let term =
            if power == 0 {
                coefficient.clone()
            } else {
                coefficient.try_mul(&generator.pow(i64::try_from(power).map_err(|_| {
                    Error::InvalidInput("series power does not fit in i64".into())
                })?)?)?
            };
        output = output.try_add(&term)?;
    }
    Ok(output)
}

fn coefficient_at_power(function: &Rat, variable: usize, power: i64) -> Result<Rat, Error> {
    if power == 0 {
        return coefficient_at_zero(function, variable);
    }
    let generator = Rat::from_poly(Poly::generator(function.ctx().clone(), variable)?);
    if power > 0 {
        coefficient_at_zero(&function.try_div(&generator.pow(power)?)?, variable)
    } else {
        let magnitude = power.checked_neg().ok_or(Error::InvalidExponent(power))?;
        coefficient_at_zero(&function.try_mul(&generator.pow(magnitude)?)?, variable)
    }
}

fn boundary_order(function: &Rat, variable: usize) -> Result<i64, Error> {
    function
        .pole_degree(variable)?
        .checked_neg()
        .ok_or_else(|| Error::InvalidInput("Laurent order cannot be negated".into()))
}

fn word_is_all_minus_one(ctx: &Arc<PolyCtx>, word: &Word) -> bool {
    if word.is_empty() {
        return false;
    }
    let minus_one = Rat::from_int(ctx.clone(), -1);
    word.letters.iter().all(|letter| letter.equal(&minus_one))
}

fn multiply_contribution(
    outer: &SymCoef,
    regulator_coefficient: &SymCoef,
    scalar: &Rat,
) -> Result<SymCoef, Error> {
    outer.try_mul(&regulator_coefficient.try_mul_rat(scalar)?)
}

fn bump_bin(
    bins: &mut BTreeMap<BinKey, RegulatorSym>,
    bin: BinKey,
    key: RegKey,
    coefficient: SymCoef,
) {
    if !coefficient.is_zero() {
        bins.entry(bin).or_default().push(RegTermSym {
            coef: coefficient,
            key,
        });
    }
}

fn effective_coefficient(entry: &ShuffleEntrySym) -> Result<SymCoef, Error> {
    let Some(factored) = &entry.factored_den else {
        return Ok(entry.coef.clone());
    };
    if !entry.coef.is_rat() {
        return Err(Error::InvalidInput(
            "factored denominator side-channel requires a rational coefficient".into(),
        ));
    }
    Ok(SymCoef::from_rat(&factored.materialize()?))
}

fn process_entry(
    ctx: &Arc<PolyCtx>,
    entry: &ShuffleEntrySym,
    variable: usize,
    check_divergences: bool,
    introduce_algebraic_letters: bool,
    remaining_variables: &[usize],
    transformed: &super::TransformResult,
) -> IntegrationResult<EntryContribution> {
    let coefficient = effective_coefficient(entry)?;
    if coefficient.is_zero() {
        return Ok(EntryContribution::default());
    }
    let mut output = EntryContribution::default();

    for monomial in coefficient.terms() {
        if monomial.prefactor.is_zero() {
            continue;
        }
        let outer = pure_symbolic_factor(ctx, monomial);
        for transformed_pair in transformed {
            let scaled = scale_wordlist(&transformed_pair.shuffle, &monomial.prefactor)?;
            let primitive = integrate_ii_with_options(
                ctx,
                &scaled,
                variable,
                &IntegrateIiOptions {
                    introduce_algebraic_letters,
                    forbidden_variables: remaining_variables,
                },
            )?;

            for primitive_term in primitive.terms {
                let rational = primitive_term.coef;
                let word = primitive_term.word;

                // Finite contribution at zero.
                let zero_order = boundary_order(&rational, variable)?;
                if zero_order >= 0 {
                    let expansion = expand_zero_word_in_context(ctx.clone(), &word, zero_order)?;
                    if let Some(row) = expansion.first().filter(|row| !row.is_empty()) {
                        let word_series = log_row_as_polynomial(ctx, row, variable)?;
                        let rational_series = series_expansion(&rational, variable, 0)?;
                        let scalar =
                            coefficient_at_zero(&word_series.try_mul(&rational_series)?, variable)?
                                .negated();
                        if !scalar.is_zero() {
                            for regulator_term in &transformed_pair.regulator {
                                output.finite.push(RegTermSym {
                                    coef: multiply_contribution(
                                        &outer,
                                        &regulator_term.coef,
                                        &scalar,
                                    )?,
                                    key: canonicalize_regkey(&regulator_term.key),
                                });
                            }
                        }
                    }
                }

                // Finite contribution at infinity.
                let reciprocal = substitute_variable_reciprocal(&rational, variable)?;
                let infinity_order = boundary_order(&reciprocal, variable)?;
                if infinity_order >= 0 {
                    if infinity_order == 0 {
                        if !word_is_all_minus_one(ctx, &word) {
                            let regularized = regzero_word_in_ctx(ctx, &word)?;
                            let limit = coefficient_at_zero(&reciprocal, variable)?;
                            if !limit.is_zero() {
                                for regulator_term in &transformed_pair.regulator {
                                    for word_term in &regularized.terms {
                                        let scalar = limit.try_mul(&word_term.coef)?;
                                        output.finite.push(RegTermSym {
                                            coef: multiply_contribution(
                                                &outer,
                                                &regulator_term.coef,
                                                &scalar,
                                            )?,
                                            key: combine_keys(
                                                &regulator_term.key,
                                                &vec![word_term.word.clone()],
                                            ),
                                        });
                                    }
                                }
                            }
                        }
                    } else {
                        let rational_series =
                            series_expansion(&reciprocal, variable, infinity_order)?;
                        let minus_one = Rat::from_int(ctx.clone(), -1);
                        let mut all_minus_one = true;
                        for split in (0..=word.len()).rev() {
                            if split < word.len() {
                                if !word[split].equal(&minus_one) {
                                    all_minus_one = false;
                                }
                                if all_minus_one {
                                    continue;
                                }
                            }
                            let prefix = Word::from(word.letters[..split].to_vec());
                            let suffix = Word::from(word.letters[split..].to_vec());
                            let expansion = expand_infinity_word_in_context(
                                ctx.clone(),
                                &prefix,
                                infinity_order,
                            )?;
                            let regularized = regzero_word_in_ctx(ctx, &suffix)?;
                            let Some(row) = expansion.first().filter(|row| !row.is_empty()) else {
                                continue;
                            };
                            if regularized.is_empty() {
                                continue;
                            }
                            let word_series = log_row_as_polynomial(ctx, row, variable)?;
                            let scalar = coefficient_at_zero(
                                &word_series.try_mul(&rational_series)?,
                                variable,
                            )?;
                            if scalar.is_zero() {
                                continue;
                            }
                            for regulator_term in &transformed_pair.regulator {
                                for word_term in &regularized.terms {
                                    let scalar = scalar.try_mul(&word_term.coef)?;
                                    output.finite.push(RegTermSym {
                                        coef: multiply_contribution(
                                            &outer,
                                            &regulator_term.coef,
                                            &scalar,
                                        )?,
                                        key: combine_keys(
                                            &regulator_term.key,
                                            &vec![word_term.word.clone()],
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }

                if !check_divergences {
                    continue;
                }

                // All non-finite bins at zero.
                if zero_order >= 0 {
                    let expansion = expand_zero_word_in_context(ctx.clone(), &word, zero_order)?;
                    let rational_series = series_expansion(&rational, variable, zero_order)?;
                    for (log_power, row) in expansion.iter().enumerate() {
                        if row.is_empty() {
                            continue;
                        }
                        let product =
                            log_row_as_polynomial(ctx, row, variable)?.try_mul(&rational_series)?;
                        for power in 0..=zero_order {
                            if log_power == 0 && power == 0 {
                                continue;
                            }
                            let scalar =
                                coefficient_at_power(&product, variable, -power)?.negated();
                            if scalar.is_zero() {
                                continue;
                            }
                            for regulator_term in &transformed_pair.regulator {
                                bump_bin(
                                    &mut output.zero_bins,
                                    (log_power as i64, power),
                                    canonicalize_regkey(&regulator_term.key),
                                    multiply_contribution(&outer, &regulator_term.coef, &scalar)?,
                                );
                            }
                        }
                    }
                }

                // All non-finite bins at infinity.
                if infinity_order >= 0 {
                    let rational_series = series_expansion(&reciprocal, variable, infinity_order)?;
                    let minus_one = Rat::from_int(ctx.clone(), -1);
                    let mut all_minus_one = true;
                    for split in (0..=word.len()).rev() {
                        if split < word.len() {
                            if !word[split].equal(&minus_one) {
                                all_minus_one = false;
                            }
                            if all_minus_one {
                                continue;
                            }
                        }
                        let prefix = Word::from(word.letters[..split].to_vec());
                        let suffix = Word::from(word.letters[split..].to_vec());
                        let expansion =
                            expand_infinity_word_in_context(ctx.clone(), &prefix, infinity_order)?;
                        let regularized = regzero_word_in_ctx(ctx, &suffix)?;
                        if expansion.is_empty() || regularized.is_empty() {
                            continue;
                        }
                        for (log_power, row) in expansion.iter().enumerate() {
                            if row.is_empty() {
                                continue;
                            }
                            let product = log_row_as_polynomial(ctx, row, variable)?
                                .try_mul(&rational_series)?;
                            for power in 0..=infinity_order {
                                if log_power == 0 && power == 0 {
                                    continue;
                                }
                                let scalar = coefficient_at_power(&product, variable, -power)?;
                                if scalar.is_zero() {
                                    continue;
                                }
                                for regulator_term in &transformed_pair.regulator {
                                    for word_term in &regularized.terms {
                                        let scalar = scalar.try_mul(&word_term.coef)?;
                                        bump_bin(
                                            &mut output.infinity_bins,
                                            (log_power as i64, power),
                                            combine_keys(
                                                &regulator_term.key,
                                                &vec![word_term.word.clone()],
                                            ),
                                            multiply_contribution(
                                                &outer,
                                                &regulator_term.coef,
                                                &scalar,
                                            )?,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(output)
}

fn positive_integer(letter: &Rat) -> Option<i64> {
    let value = letter.to_string().parse::<i64>().ok()?;
    (value > 0).then_some(value)
}

fn word_has_positive_letter(word: &Word) -> bool {
    word.letters
        .iter()
        .any(|letter| positive_integer(letter).is_some())
}

/// Apply HyperFLINT's final positive-axis contour closure.
pub fn close_positive_letters(
    ctx: &Arc<PolyCtx>,
    regulator: &RegulatorSym,
    variable: usize,
    table: &MzvReductionTable,
) -> IntegrationResult<RegulatorSym> {
    let variable_name = ctx
        .vars()
        .get(variable)
        .ok_or_else(|| Error::UnknownVariable(variable.to_string()))?;
    let mut output = RegulatorSym::new();

    for term in regulator {
        let (with_positive, without_positive): (Vec<_>, Vec<_>) =
            term.key.iter().cloned().partition(word_has_positive_letter);
        if with_positive.len() != 1 {
            output.push(RegTermSym {
                coef: term.coef.clone(),
                key: canonicalize_regkey(&term.key),
            });
            continue;
        }

        let word = &with_positive[0];
        let mut positive_letters = word
            .letters
            .iter()
            .filter_map(|letter| positive_integer(letter).map(|value| (value, letter.clone())))
            .collect::<Vec<_>>();
        positive_letters.sort_by_key(|(value, _)| *value);
        positive_letters.dedup_by_key(|(value, _)| *value);
        let on_axis = positive_letters
            .into_iter()
            .map(|(_, letter)| OnAxisSymEntry {
                letter,
                im_part: SymCoef::delta_factor(ctx.clone(), variable_name.clone()),
            })
            .collect::<Vec<_>>();
        let seed = WordlistSym {
            terms: vec![WordlistSymTerm {
                coef: SymCoef::one(ctx.clone()),
                word: word.clone(),
            }],
        };
        for contour_term in break_up_contour_sym(ctx, &seed, &on_axis, table)? {
            output.push(RegTermSym {
                coef: term.coef.try_mul(&contour_term.coef)?,
                key: combine_keys(&without_positive, &contour_term.key),
            });
        }
    }
    Ok(canonicalize_regulator_sym(&output)?)
}

fn regulator_bin_is_zero(
    ctx: &Arc<PolyCtx>,
    regulator: &RegulatorSym,
    remaining_variables: &[usize],
    table: &MzvReductionTable,
) -> IntegrationResult<bool> {
    let canonical = canonicalize_regulator_sym(regulator)?;
    if canonical.is_empty() {
        return Ok(true);
    }
    Ok(test_zero_function_sym(
        ctx,
        &canonical,
        remaining_variables,
        table,
    )?)
}

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
pub(super) fn integration_step_core_sym_with_options(
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
    let mut transform_cache = HashMap::<String, Arc<super::TransformResult>>::new();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> MzvReductionTable {
        MzvReductionTable::default()
    }

    #[test]
    fn convergent_double_pole_integrates_to_one() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let input = vec![ShuffleEntry::new(
            Rat::parse(ctx.clone(), "1/(x+1)^2").unwrap(),
            Vec::new(),
        )];
        let result = integration_step(&ctx, &input, 0, &table(), true).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].key.is_empty());
        assert_eq!(result[0].coef.as_rat().unwrap(), Rat::one(ctx));
    }

    #[test]
    fn divergence_is_a_structured_error() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let input = vec![ShuffleEntry::new(
            Rat::parse(ctx.clone(), "1/(x+1)").unwrap(),
            Vec::new(),
        )];
        let error = integration_step(&ctx, &input, 0, &table(), true).unwrap_err();
        assert!(matches!(
            error,
            IntegrationError::Divergent {
                boundary: Boundary::Infinity,
                ..
            }
        ));
    }

    #[test]
    fn factored_input_is_materialized_without_losing_the_denominator() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let mut entry = ShuffleEntry::new(Rat::one(ctx.clone()), Vec::new());
        entry.factored_den = Some(FactoredRat::parse(ctx.clone(), "1/(x+1)^2").unwrap());
        let result = integration_step(&ctx, &vec![entry], 0, &table(), false).unwrap();
        assert_eq!(result[0].coef.as_rat().unwrap(), Rat::one(ctx));
    }
}
