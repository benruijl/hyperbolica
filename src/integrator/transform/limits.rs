use std::sync::Arc;

use symbolica::prelude::Rational;

use super::collection::{
    canonicalize_regkey, canonicalize_regulator_sym, require_word_context, shuffle_symbolic_sym,
};
use super::{RegKey, RegTermSym, RegulatorSym};
use crate::core::{PolyCtx, Rat, SymCoef};
use crate::error::{Error, Result};
use crate::reduce::{
    MzvReductionTable, OnAxisSymEntry, WordlistSym, WordlistSymTerm, break_up_contour_sym,
};
use crate::symbols::Word;

use crate::integrator::regularize::regzero_word_in_ctx;

pub(super) fn word_depends_on_variable(word: &Word, variable: usize) -> Result<bool> {
    for letter in &word.letters {
        if letter.depends_on(variable)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn all_letters_equal_integer(word: &Word, value: i64) -> bool {
    if word.is_empty() {
        return false;
    }
    let expected = Rat::from_int(word[0].ctx().clone(), value);
    word.letters.iter().all(|letter| letter.equal(&expected))
}

fn all_letters_in_period_scope(word: &Word) -> bool {
    !word.is_empty()
        && word.letters.iter().all(|letter| {
            letter
                .integer_constant()
                .and_then(|value| value.to_i64())
                .is_some_and(|value| matches!(value, -2..=0))
        })
}

/// Return distinct positive integer letters in ascending value order while
/// retaining the index of each value's first occurrence. The original index
/// determines the contour side when the leading Laurent order is zero.
pub(super) fn first_positive_letters(word: &Word) -> Vec<(i64, usize, Rat)> {
    let mut positive_letters = Vec::new();
    for (index, letter) in word.letters.iter().enumerate() {
        let Some(value) = letter
            .integer_constant()
            .and_then(|value| value.to_i64())
            .filter(|value| *value > 0)
        else {
            continue;
        };
        if !positive_letters.iter().any(|(seen, _, _)| *seen == value) {
            positive_letters.push((value, index, letter.clone()));
        }
    }
    positive_letters.sort_by_key(|(value, _, _)| *value);
    positive_letters
}

pub(super) fn one_regulator(ctx: &Arc<PolyCtx>, key: RegKey) -> RegulatorSym {
    vec![RegTermSym {
        coef: SymCoef::one(ctx.clone()),
        key: canonicalize_regkey(&key),
    }]
}

/// Compute the regularized limit at `variable = 0`.
///
/// Rational scaling and trailing-zero decomposition are evaluated exactly.
/// Periods that require the separate MZV/contour layer remain as symbolic
/// words in the regulator key, which is the shape-preserving HyperFLINT
/// phase-5 contract.
pub fn reglim_word(ctx: &Arc<PolyCtx>, word: &Word, variable: usize) -> Result<RegulatorSym> {
    if variable >= ctx.len() {
        return Err(Error::UnknownVariable(variable.to_string()));
    }
    require_word_context(word, ctx)?;
    reglim_word_impl(ctx, word, variable, None)
}

/// Compute the regularized limit with the active MZV reduction table.
///
/// The table enables HyperFLINT's two value-changing limit branches: literal
/// `{-2,-1,0}` periods are evaluated, and positive real letters are continued
/// around the integration contour with their formal `delta[variable]` side.
pub fn reglim_word_with_table(
    ctx: &Arc<PolyCtx>,
    word: &Word,
    variable: usize,
    table: &MzvReductionTable,
) -> Result<RegulatorSym> {
    if variable >= ctx.len() {
        return Err(Error::UnknownVariable(variable.to_string()));
    }
    require_word_context(word, ctx)?;
    reglim_word_impl(ctx, word, variable, Some(table))
}

pub(super) fn reglim_word_impl(
    ctx: &Arc<PolyCtx>,
    word: &Word,
    variable: usize,
    table: Option<&MzvReductionTable>,
) -> Result<RegulatorSym> {
    if word.is_empty() {
        return Ok(one_regulator(ctx, Vec::new()));
    }

    if !word_depends_on_variable(word, variable)? {
        if all_letters_equal_integer(word, 0) || all_letters_equal_integer(word, -1) {
            return Ok(Vec::new());
        }
        if let Some(table) = table.filter(|_| all_letters_in_period_scope(word)) {
            return break_up_contour_sym(
                ctx,
                &WordlistSym {
                    terms: vec![WordlistSymTerm {
                        coef: SymCoef::one(ctx.clone()),
                        word: word.clone(),
                    }],
                },
                &[],
                table,
            );
        }
        return Ok(one_regulator(ctx, vec![word.clone()]));
    }

    let zero_orders = word
        .letters
        .iter()
        .map(|letter| letter.pole_degree(variable))
        .collect::<Result<Vec<_>>>()?;
    let minimum_order = *zero_orders
        .iter()
        .min()
        .expect("a non-empty word has at least one Laurent order");
    let trailing = zero_orders
        .iter()
        .rev()
        .take_while(|order| **order > minimum_order)
        .count();

    if trailing > 0 {
        let head_length = word.len() - trailing;
        let head_base = Word::from(word.letters[..head_length].to_vec());
        let mut output = RegulatorSym::new();
        for appended_zeros in 0..=trailing {
            let mut head = head_base.clone();
            head.letters
                .extend((0..appended_zeros).map(|_| Rat::zero(ctx.clone())));
            let regularized_head = regzero_word_in_ctx(ctx, &head)?;
            let mut head_regulator = RegulatorSym::new();
            for term in regularized_head.terms {
                for subterm in reglim_word_impl(ctx, &term.word, variable, table)? {
                    head_regulator.push(RegTermSym {
                        coef: subterm.coef.try_mul_rat(&term.coef)?,
                        key: subterm.key,
                    });
                }
            }
            let head_regulator = canonicalize_regulator_sym(&head_regulator)?;

            let tail_start = head_length + appended_zeros;
            let tail = if tail_start < word.len() {
                Word::from(word.letters[tail_start..].to_vec())
            } else {
                Word::default()
            };
            let tail_regulator = reglim_word_impl(ctx, &tail, variable, table)?;
            output.extend(shuffle_symbolic_sym(&head_regulator, &tail_regulator)?);
        }
        return canonicalize_regulator_sym(&output);
    }

    let mut scaled = Word::default();
    for (letter, zero_order) in word.letters.iter().zip(zero_orders) {
        if zero_order > minimum_order {
            scaled.letters.push(Rat::zero(ctx.clone()));
        } else if !letter.depends_on(variable)? {
            scaled.letters.push(letter.clone());
        } else {
            scaled.letters.push(letter.residue(variable)?);
        }
    }

    if all_letters_equal_integer(&scaled, 0) || all_letters_equal_integer(&scaled, -1) {
        return Ok(Vec::new());
    }
    if let Some(table) = table.filter(|_| all_letters_in_period_scope(&scaled)) {
        return break_up_contour_sym(
            ctx,
            &WordlistSym {
                terms: vec![WordlistSymTerm {
                    coef: SymCoef::one(ctx.clone()),
                    word: scaled,
                }],
            },
            &[],
            table,
        );
    }
    if let Some(table) = table {
        // Pinned HyperFLINT keeps the first occurrence of each positive
        // value because its original letter determines the contour side.
        // Deduplicate before sorting so equal scaled letters with distinct
        // subleading terms cannot select a later occurrence accidentally.
        let positive_letters = first_positive_letters(&scaled);

        if !positive_letters.is_empty() {
            let delta = SymCoef::delta_factor(ctx.clone(), variable)?;
            let ones = vec![Rational::one(); ctx.len()];
            let on_axis = positive_letters
                .into_iter()
                .map(|(_, original_index, letter)| {
                    let im_part = if minimum_order > 0 {
                        delta.clone()
                    } else if minimum_order < 0 {
                        delta.negated()
                    } else {
                        let next_coefficient =
                            word[original_index].try_sub(&letter)?.residue(variable)?;
                        // This vars-at-one sign sample intentionally mirrors
                        // the pinned C++ compatibility implementation. It is
                        // a deterministic real-domain heuristic, not a proof
                        // of the sign over the full parameter domain.
                        match next_coefficient.evaluate_rational(&ones) {
                            Ok(value) if value.is_negative() => delta.negated(),
                            _ => delta.clone(),
                        }
                    };
                    Ok(OnAxisSymEntry { letter, im_part })
                })
                .collect::<Result<Vec<_>>>()?;
            return break_up_contour_sym(
                ctx,
                &WordlistSym {
                    terms: vec![WordlistSymTerm {
                        coef: SymCoef::one(ctx.clone()),
                        word: scaled,
                    }],
                },
                &on_axis,
                table,
            );
        }
    }
    Ok(one_regulator(ctx, vec![scaled]))
}
