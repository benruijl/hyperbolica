use std::sync::Arc;

use super::IntegrationResult;
use super::entry::combine_keys;
use crate::core::{PolyCtx, Rat, SymCoef};
use crate::error::Error;
use crate::integrator::{
    RegTermSym, RegulatorSym, canonicalize_regkey, canonicalize_regulator_sym,
};
use crate::reduce::{
    MzvReductionTable, OnAxisSymEntry, WordlistSym, WordlistSymTerm, break_up_contour_sym,
    test_zero_function_sym,
};
use crate::symbols::Word;

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

pub(super) fn regulator_bin_is_zero(
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
