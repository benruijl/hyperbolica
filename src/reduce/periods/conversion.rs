use std::sync::Arc;

use crate::core::{Poly, PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::symbols::{Word, Wordlist, mzv_atom};

use crate::reduce::mzv_expansion::{MzvExpansionTable, cross_ctx_transfer_rat};

pub(super) fn integer_letter(letter: &Rat, site: &str) -> Result<i64> {
    letter
        .integer_constant()
        .and_then(|value| value.to_i64())
        .ok_or_else(|| Error::InvalidInput(format!("{site}: non-integer letter `{letter}`")))
}

fn mzv_name(indices: &[i64]) -> String {
    let encoded = indices
        .iter()
        .map(|index| {
            if *index < 0 {
                format!("m{}", index.unsigned_abs())
            } else {
                index.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("_");
    format!("mzv_{encoded}")
}

fn to_mzv_one_word(
    ctx: &Arc<PolyCtx>,
    coefficient: &Rat,
    word: &Word,
    expansion: Option<&MzvExpansionTable>,
) -> Result<Rat> {
    if word.is_empty() {
        return Ok(coefficient.clone());
    }
    if word[0].is_one() || word[word.len() - 1].is_zero() {
        // The period entry points regularize these cases before conversion.
        return Ok(Rat::zero(ctx.clone()));
    }

    let mut counts = Vec::<i64>::new();
    let mut poles = Vec::<i64>::new();
    for letter in word.letters.iter().rev() {
        if letter.is_zero() {
            if let Some(count) = counts.last_mut() {
                *count += 1;
            }
        } else {
            counts.push(1);
            poles.push(integer_letter(letter, "to_mzv")?);
        }
    }
    if counts.is_empty() {
        return Ok(Rat::zero(ctx.clone()));
    }
    poles.push(1);

    let mut indices = Vec::with_capacity(counts.len());
    for index in 0..counts.len() {
        let numerator = counts[index]
            .checked_mul(poles[index + 1])
            .ok_or_else(|| Error::InvalidInput("to_mzv: index overflow".into()))?;
        let denominator = poles[index];
        if denominator == 0 || numerator % denominator != 0 {
            return Err(Error::InvalidInput(
                "to_mzv: non-integral MZV index (unsupported alphabet)".into(),
            ));
        }
        indices.push(numerator / denominator);
    }

    let name = mzv_name(&indices);
    let native_atom = mzv_atom(&indices);
    let symbol = if let Some(variable) = ctx.index_of_indeterminate(native_atom.as_view()) {
        Rat::from_poly(Poly::generator(ctx.clone(), variable)?)
    } else if let Some(value) = expansion.and_then(|table| table.expansion.get(&name)) {
        cross_ctx_transfer_rat(value, ctx.clone())?
    } else {
        return Err(Error::UnknownVariable(format!(
            "{name} (required by to_mzv)"
        )));
    };
    let value = coefficient.try_mul(&symbol)?;
    Ok(if counts.len() & 1 == 0 {
        value
    } else {
        value.negated()
    })
}

pub(super) fn to_mzv(ctx: &Arc<PolyCtx>, wordlist: &Wordlist) -> Result<Rat> {
    to_mzv_with_expansion(ctx, wordlist, None)
}

pub(super) fn to_mzv_with_expansion(
    ctx: &Arc<PolyCtx>,
    wordlist: &Wordlist,
    expansion: Option<&MzvExpansionTable>,
) -> Result<Rat> {
    let mut result = Rat::zero(ctx.clone());
    for term in &wordlist.terms {
        result = result.try_add(&to_mzv_one_word(ctx, &term.coef, &term.word, expansion)?)?;
    }
    Ok(result)
}
