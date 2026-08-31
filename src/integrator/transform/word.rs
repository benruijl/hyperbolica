use std::collections::HashMap;
use std::sync::Arc;

use super::collection::{
    canonicalize_regulator_sym, regulator_sym_content_key, require_word_context,
};
use super::limits::{one_regulator, reglim_word_impl, word_depends_on_variable};
use super::{RegulatorSym, TransformPair, TransformResult};
use crate::algebra::linear_factors::linear_factors;
use crate::core::{PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::symbols::{Word, Wordlist, WordlistTerm};

fn trailing_zero(word: &Word) -> bool {
    word.letters.last().is_some_and(Rat::is_zero)
}

pub(super) fn identity_transform(ctx: &Arc<PolyCtx>) -> TransformResult {
    vec![TransformPair {
        shuffle: Wordlist::from(vec![WordlistTerm::new(
            Rat::one(ctx.clone()),
            Word::default(),
        )]),
        regulator: one_regulator(ctx, Vec::new()),
    }]
}

struct ResultRow {
    regulator: RegulatorSym,
    word_indices: HashMap<String, usize>,
    terms: Vec<WordlistTerm>,
}

fn bump_result(
    rows: &mut Vec<ResultRow>,
    row_indices: &mut HashMap<String, usize>,
    regulator: &RegulatorSym,
    word: Word,
    coefficient: Rat,
) -> Result<()> {
    let regulator_key = regulator_sym_content_key(regulator)?;
    let row_index = if let Some(&index) = row_indices.get(&regulator_key) {
        index
    } else {
        let index = rows.len();
        row_indices.insert(regulator_key, index);
        rows.push(ResultRow {
            regulator: canonicalize_regulator_sym(regulator)?,
            word_indices: HashMap::new(),
            terms: Vec::new(),
        });
        index
    };

    let row = &mut rows[row_index];
    let word_key = word.content_key();
    if let Some(&index) = row.word_indices.get(&word_key) {
        row.terms[index].coef = row.terms[index].coef.try_add(&coefficient)?;
    } else {
        row.word_indices.insert(word_key, row.terms.len());
        row.terms.push(WordlistTerm::new(coefficient, word));
    }
    Ok(())
}

#[derive(Clone)]
struct SignedLinearFactor {
    multiplicity: i64,
    pole: Rat,
}

type TransformCache = HashMap<String, TransformResult>;

fn transform_cache_key(word: &Word, variable: usize) -> String {
    format!("{variable}|{}", word.content_key())
}

/// Transform one word into shuffle factors and regularized limits.
pub fn transform_word(ctx: &Arc<PolyCtx>, word: &Word, variable: usize) -> Result<TransformResult> {
    if variable >= ctx.len() {
        return Err(Error::UnknownVariable(variable.to_string()));
    }
    require_word_context(word, ctx)?;
    transform_word_impl(ctx, word, variable, &mut HashMap::new())
}

fn transform_word_impl(
    ctx: &Arc<PolyCtx>,
    word: &Word,
    variable: usize,
    cache: &mut TransformCache,
) -> Result<TransformResult> {
    let cache_key = transform_cache_key(word, variable);
    if let Some(cached) = cache.get(&cache_key) {
        return Ok(cached.clone());
    }
    if word.is_empty() {
        let output = identity_transform(ctx);
        cache.insert(cache_key, output.clone());
        return Ok(output);
    }

    let limit = reglim_word_impl(ctx, word, variable)?;
    let has_variable = word_depends_on_variable(word, variable)?;
    let mut rows = Vec::<ResultRow>::new();
    let mut row_indices = HashMap::<String, usize>::new();
    if !limit.is_empty() && has_variable {
        bump_result(
            &mut rows,
            &mut row_indices,
            &limit,
            Word::default(),
            Rat::one(ctx.clone()),
        )?;
    }

    if !has_variable {
        let output = if limit.is_empty() {
            Vec::new()
        } else {
            vec![TransformPair {
                shuffle: Wordlist::from(vec![WordlistTerm::new(
                    Rat::one(ctx.clone()),
                    Word::default(),
                )]),
                regulator: canonicalize_regulator_sym(&limit)?,
            }]
        };
        cache.insert(cache_key, output.clone());
        return Ok(output);
    }
    if trailing_zero(word) {
        return Err(Error::InvalidInput("TransformWord: $Failed".into()));
    }

    for index in 0..word.len() {
        if index == word.len() - 1 && word.len() >= 2 && word[word.len() - 2].is_zero() {
            break;
        }
        let difference = if index + 1 < word.len() {
            word[index].try_sub(&word[index + 1])?
        } else {
            word[index].clone()
        };
        if difference.is_zero() {
            continue;
        }

        let numerator = linear_factors(difference.numerator(), variable)?;
        let denominator = linear_factors(difference.denominator(), variable)?;
        let mut factors = Vec::<SignedLinearFactor>::new();
        for factor in numerator.linear {
            factors.push(SignedLinearFactor {
                multiplicity: i64::try_from(factor.multiplicity).map_err(|_| {
                    Error::InvalidInput("linear-factor multiplicity does not fit in i64".into())
                })?,
                pole: factor.pole,
            });
        }
        for factor in denominator.linear {
            factors.push(SignedLinearFactor {
                multiplicity: -i64::try_from(factor.multiplicity).map_err(|_| {
                    Error::InvalidInput("linear-factor multiplicity does not fit in i64".into())
                })?,
                pole: factor.pole,
            });
        }
        if factors.is_empty() {
            continue;
        }

        if index + 1 < word.len() {
            let mut letters = Vec::with_capacity(word.len() - 1);
            letters.extend(word.letters[..=index].iter().cloned());
            letters.extend(word.letters[index + 2..].iter().cloned());
            let subword = Word::from(letters);
            if !subword.is_empty() && !trailing_zero(&subword) {
                let transformed = transform_word_impl(ctx, &subword, variable, cache)?;
                append_factored_rows(&mut rows, &mut row_indices, &transformed, &factors, 1, ctx)?;
            }
        }

        let mut letters = Vec::with_capacity(word.len() - 1);
        letters.extend(word.letters[..index].iter().cloned());
        letters.extend(word.letters[index + 1..].iter().cloned());
        let subword = Word::from(letters);
        if subword.is_empty() || !trailing_zero(&subword) {
            let transformed = transform_word_impl(ctx, &subword, variable, cache)?;
            append_factored_rows(&mut rows, &mut row_indices, &transformed, &factors, -1, ctx)?;
        }
    }

    let output = rows
        .into_iter()
        .filter_map(|mut row| {
            row.terms.retain(|term| !term.coef.is_zero());
            (!row.terms.is_empty()).then_some(TransformPair {
                shuffle: Wordlist::from(row.terms),
                regulator: row.regulator,
            })
        })
        .collect::<Vec<_>>();
    cache.insert(cache_key, output.clone());
    Ok(output)
}

fn append_factored_rows(
    rows: &mut Vec<ResultRow>,
    row_indices: &mut HashMap<String, usize>,
    transformed: &TransformResult,
    factors: &[SignedLinearFactor],
    sign: i64,
    ctx: &Arc<PolyCtx>,
) -> Result<()> {
    for pair in transformed {
        for term in &pair.shuffle.terms {
            for factor in factors {
                let mut letters = Vec::with_capacity(term.word.len() + 1);
                letters.push(factor.pole.clone());
                letters.extend(term.word.letters.iter().cloned());
                let signed_multiplicity =
                    factor.multiplicity.checked_mul(sign).ok_or_else(|| {
                        Error::InvalidInput("signed linear-factor multiplicity overflowed".into())
                    })?;
                let coefficient =
                    Rat::from_int(ctx.clone(), signed_multiplicity).try_mul(&term.coef)?;
                bump_result(
                    rows,
                    row_indices,
                    &pair.regulator,
                    Word::from(letters),
                    coefficient,
                )?;
            }
        }
    }
    Ok(())
}
