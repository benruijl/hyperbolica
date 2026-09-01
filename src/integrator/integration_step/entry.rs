use std::collections::BTreeMap;
use std::sync::Arc;

use super::{IntegrationResult, ShuffleEntrySym};
use crate::core::{Poly, PolyCtx, Rat, SymCoef, SymMonomial};
use crate::error::Error;
use crate::integrator::{
    IntegrateIiOptions, RegKey, RegTermSym, RegulatorSym, TransformResult, canonicalize_regkey,
    integrate_ii_with_factored_prefactor, integrate_ii_with_options, regzero_word_in_ctx,
};
use crate::series::expansions::{expand_infinity_word_in_context, expand_zero_word_in_context};
use crate::series::laurent::{
    coefficient_at_zero, series_expansion, substitute_variable_reciprocal,
};
use crate::symbols::{Word, Wordlist, WordlistTerm};

pub(super) type BinKey = (i64, i64);

#[derive(Default)]
pub(super) struct EntryContribution {
    pub(super) finite: RegulatorSym,
    pub(super) zero_bins: BTreeMap<BinKey, RegulatorSym>,
    pub(super) infinity_bins: BTreeMap<BinKey, RegulatorSym>,
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

pub(super) fn combine_keys(left: &RegKey, right: &RegKey) -> RegKey {
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
    let Some(factored) = &entry.factored_coefficient else {
        return Ok(entry.coef.clone());
    };
    if !entry.coef.is_one() {
        return Err(Error::InvalidInput(
            "a factored ShuffleEntry requires its `coef` sentinel to remain one".into(),
        ));
    }
    // The deferred value is the complete coefficient. The unit sentinel keeps
    // the existing symbolic outer loop while the primitive layer performs the
    // first blockwise partial-fraction step.
    Ok(SymCoef::one(factored.ctx().clone()))
}

pub(super) fn process_entry(
    ctx: &Arc<PolyCtx>,
    entry: &ShuffleEntrySym,
    variable: usize,
    check_divergences: bool,
    introduce_algebraic_letters: bool,
    forbidden_algebraic_variables: &[usize],
    transformed: &TransformResult,
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
            let primitive_options = IntegrateIiOptions {
                introduce_algebraic_letters,
                forbidden_variables: forbidden_algebraic_variables,
            };
            let primitive = if let Some(factored) = &entry.factored_coefficient {
                integrate_ii_with_factored_prefactor(
                    ctx,
                    &transformed_pair.shuffle,
                    factored,
                    variable,
                    &primitive_options,
                )?
            } else {
                let scaled = scale_wordlist(&transformed_pair.shuffle, &monomial.prefactor)?;
                integrate_ii_with_options(ctx, &scaled, variable, &primitive_options)?
            };

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
