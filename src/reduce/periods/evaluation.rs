use std::collections::BTreeSet;
use std::sync::Arc;

use super::conversion::{integer_letter, to_mzv_with_expansion};
use crate::algebra::convert::{convert_one_infinity_to_zero_one, convert_zero_one};
use crate::core::{Poly, PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::integrator::{RegKey, RegTerm, Regulator, canonicalize_regulator, reg_head, reg0};
use crate::symbols::{Word, Wordlist, WordlistTerm, log_two_atom};

use crate::reduce::mzv_expansion::MzvExpansionTable;
use crate::reduce::mzv_reduce::{MzvReductionTable, apply_mzv_reductions};

fn all_zero(word: &Word) -> bool {
    !word.is_empty() && word.letters.iter().all(Rat::is_zero)
}

pub(super) fn zero_one_period(
    ctx: &Arc<PolyCtx>,
    word: &Word,
    table: &MzvReductionTable,
) -> Result<Rat> {
    zero_one_period_with_expansion(ctx, word, table, None)
}

pub(super) fn zero_one_period_with_expansion(
    ctx: &Arc<PolyCtx>,
    word: &Word,
    table: &MzvReductionTable,
    expansion: Option<&MzvExpansionTable>,
) -> Result<Rat> {
    if word.is_empty() {
        return Ok(Rat::one(ctx.clone()));
    }
    if all_zero(word) {
        return Ok(Rat::zero(ctx.clone()));
    }

    if word[word.len() - 1].is_zero() {
        let seed = Wordlist::new(vec![WordlistTerm::new(Rat::one(ctx.clone()), word.clone())]);
        let regularized = reg0(&seed)?;
        let mut result = Rat::zero(ctx.clone());
        for term in regularized.terms {
            let period = zero_one_period_with_expansion(ctx, &term.word, table, expansion)?;
            result = result.try_add(&term.coef.try_mul(&period)?)?;
        }
        return Ok(result);
    }

    if word[0].to_string() == "1" {
        let seed = Wordlist::new(vec![WordlistTerm::new(Rat::one(ctx.clone()), word.clone())]);
        let regularized = reg_head(&seed, &Rat::one(ctx.clone()), &Rat::zero(ctx.clone()))?;
        let mut result = Rat::zero(ctx.clone());
        for term in regularized.terms {
            let period = zero_one_period_with_expansion(ctx, &term.word, table, expansion)?;
            result = result.try_add(&term.coef.try_mul(&period)?)?;
        }
        return Ok(result);
    }

    for letter in &word.letters {
        if !matches!(integer_letter(letter, "zero_one_period")?, -1..=1) {
            return Err(Error::InvalidInput(
                "zero_one_period: letter outside {-1,0,1}".into(),
            ));
        }
    }
    let seed = Wordlist::new(vec![WordlistTerm::new(Rat::one(ctx.clone()), word.clone())]);
    let raw = to_mzv_with_expansion(ctx, &seed, expansion)?;
    apply_mzv_reductions(table, &raw)
}

pub(super) fn zero_inf_period(
    ctx: &Arc<PolyCtx>,
    word: &Word,
    table: &MzvReductionTable,
) -> Result<Rat> {
    zero_inf_period_with_expansion(ctx, word, table, None)
}

pub(super) fn zero_inf_period_with_expansion(
    ctx: &Arc<PolyCtx>,
    word: &Word,
    table: &MzvReductionTable,
    expansion: Option<&MzvExpansionTable>,
) -> Result<Rat> {
    if word.is_empty() {
        return Ok(Rat::one(ctx.clone()));
    }
    if all_zero(word) {
        return Ok(Rat::zero(ctx.clone()));
    }

    let mut nonzero = Vec::new();
    let mut has_zero = false;
    for letter in &word.letters {
        if letter.is_zero() {
            has_zero = true;
        } else {
            nonzero.push(integer_letter(letter, "zero_inf_period")?);
        }
    }
    if !has_zero && !nonzero.is_empty() && nonzero.iter().all(|value| *value == -1) {
        return Ok(Rat::zero(ctx.clone()));
    }

    if word[word.len() - 1].is_zero() {
        let seed = Wordlist::new(vec![WordlistTerm::new(Rat::one(ctx.clone()), word.clone())]);
        let regularized = reg0(&seed)?;
        let mut result = Rat::zero(ctx.clone());
        for term in regularized.terms {
            let period = zero_inf_period_with_expansion(ctx, &term.word, table, expansion)?;
            result = result.try_add(&term.coef.try_mul(&period)?)?;
        }
        return Ok(result);
    }

    if nonzero.iter().all(|value| *value == -1) {
        let converted = convert_zero_one(&Wordlist::new(vec![WordlistTerm::new(
            Rat::one(ctx.clone()),
            word.clone(),
        )]))?;
        let mut result = Rat::zero(ctx.clone());
        for term in converted.terms {
            let period = zero_one_period_with_expansion(ctx, &term.word, table, expansion)?;
            result = result.try_add(&term.coef.try_mul(&period)?)?;
        }
        return Ok(result);
    }

    let unique = nonzero.iter().copied().collect::<BTreeSet<_>>();
    if unique.len() == 1 {
        let letter = *unique.first().expect("one distinct nonzero letter");
        if letter != -2 {
            return Err(Error::InvalidInput(format!(
                "zero_inf_period: single-letter rescaling supports -2, got {letter}"
            )));
        }
        let scaled = Word::new(
            word.letters
                .iter()
                .map(|value| {
                    if value.is_zero() {
                        Ok(Rat::zero(ctx.clone()))
                    } else {
                        Ok(Rat::from_int(
                            ctx.clone(),
                            integer_letter(value, "zero_inf_period")? / 2,
                        ))
                    }
                })
                .collect::<Result<Vec<_>>>()?,
        );
        let log2 = log_two_atom();
        let log2_index = ctx
            .index_of_indeterminate(log2.as_view())
            .ok_or_else(|| Error::UnknownVariable("Log2".into()))?;
        let negative_log2 = Rat::from_poly(Poly::generator(ctx.clone(), log2_index)?).negated();
        let mut log_factor = Rat::one(ctx.clone());
        let mut result = Rat::zero(ctx.clone());
        for offset in 0..=scaled.len() {
            let tail = Word::new(scaled.letters[offset..].to_vec());
            let period = zero_inf_period_with_expansion(ctx, &tail, table, expansion)?;
            result = result.try_add(&log_factor.try_mul(&period)?)?;
            if offset < scaled.len() {
                log_factor = log_factor
                    .try_mul(&negative_log2)?
                    .try_div(&Rat::from_int(ctx.clone(), (offset + 1) as i64))?;
            }
        }
        return Ok(result);
    }

    if unique == BTreeSet::from([-2, -1]) {
        let shifted = Word::new(
            word.letters
                .iter()
                .map(|letter| {
                    if letter.is_zero() {
                        Ok(Rat::one(ctx.clone()))
                    } else {
                        Ok(Rat::from_int(
                            ctx.clone(),
                            integer_letter(letter, "zero_inf_period")? + 1,
                        ))
                    }
                })
                .collect::<Result<Vec<_>>>()?,
        );
        let converted = convert_one_infinity_to_zero_one(&Wordlist::new(vec![WordlistTerm::new(
            Rat::one(ctx.clone()),
            shifted,
        )]))?;
        let mut result = Rat::zero(ctx.clone());
        for term in converted.terms {
            let period = zero_one_period_with_expansion(ctx, &term.word, table, expansion)?;
            result = result.try_add(&term.coef.try_mul(&period)?)?;
        }
        return Ok(result);
    }

    Err(Error::InvalidInput(
        "zero_inf_period: letters outside the {-2,-1,0} MZV scope".into(),
    ))
}

pub(super) fn evaluate_periods(
    ctx: &Arc<PolyCtx>,
    regulator: &Regulator,
    table: &MzvReductionTable,
) -> Result<Regulator> {
    let mut constant = Rat::zero(ctx.clone());
    let mut passthrough = Regulator::new();
    for term in regulator {
        if term.key.is_empty() {
            constant = constant.try_add(&term.coef)?;
            continue;
        }

        let mut product = Rat::one(ctx.clone());
        let mut evaluable = true;
        for word in &term.key {
            match zero_inf_period(ctx, word, table) {
                Ok(period) => product = product.try_mul(&period)?,
                Err(_) => {
                    evaluable = false;
                    break;
                }
            }
        }
        if evaluable {
            constant = constant.try_add(&term.coef.try_mul(&product)?)?;
        } else {
            passthrough.push(term.clone());
        }
    }

    let mut output = Regulator::new();
    if !constant.is_zero() {
        output.push(RegTerm {
            coef: constant,
            key: RegKey::new(),
        });
    }
    output.extend(passthrough);
    canonicalize_regulator(&output)
}

pub(super) fn test_zero_function(
    ctx: &Arc<PolyCtx>,
    regulator: &Regulator,
    table: &MzvReductionTable,
) -> Result<Rat> {
    let reduced = evaluate_periods(ctx, regulator, table)?;
    let mut total = Rat::zero(ctx.clone());
    for term in reduced {
        total = total.try_add(&term.coef)?;
    }
    Ok(total)
}
