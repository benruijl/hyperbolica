use std::sync::Arc;

use super::collection::{
    canonicalize_regkey, canonicalize_regulator_sym, require_word_context, shuffle_symbolic_sym,
};
use super::{RegKey, RegTermSym, RegulatorSym};
use crate::core::{PolyCtx, Rat, SymCoef};
use crate::error::{Error, Result};
use crate::symbols::Word;

use crate::integrator::regularize::regzero_word_in_ctx;

pub(super) fn word_depends_on_variable(word: &Word, variable: usize) -> Result<bool> {
    for letter in &word.letters {
        if !letter.derivative(variable)?.is_zero() {
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
    reglim_word_impl(ctx, word, variable)
}

pub(super) fn reglim_word_impl(
    ctx: &Arc<PolyCtx>,
    word: &Word,
    variable: usize,
) -> Result<RegulatorSym> {
    if word.is_empty() {
        return Ok(one_regulator(ctx, Vec::new()));
    }

    if !word_depends_on_variable(word, variable)? {
        if all_letters_equal_integer(word, 0) || all_letters_equal_integer(word, -1) {
            return Ok(Vec::new());
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
                for subterm in reglim_word_impl(ctx, &term.word, variable)? {
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
            let tail_regulator = reglim_word_impl(ctx, &tail, variable)?;
            output.extend(shuffle_symbolic_sym(&head_regulator, &tail_regulator)?);
        }
        return canonicalize_regulator_sym(&output);
    }

    let mut scaled = Word::default();
    for (letter, zero_order) in word.letters.iter().zip(zero_orders) {
        if zero_order > minimum_order {
            scaled.letters.push(Rat::zero(ctx.clone()));
        } else if letter.derivative(variable)?.is_zero() {
            scaled.letters.push(letter.clone());
        } else {
            scaled.letters.push(letter.residue(variable)?);
        }
    }

    if all_letters_equal_integer(&scaled, 0) || all_letters_equal_integer(&scaled, -1) {
        return Ok(Vec::new());
    }
    Ok(one_regulator(ctx, vec![scaled]))
}
