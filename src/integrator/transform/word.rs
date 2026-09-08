use std::sync::Arc;

use super::collection::{
    canonicalize_regulator_sym, regulator_sym_bucket_digest, regulator_sym_structurally_equal,
    require_word_context,
};
use super::limits::{one_regulator, reglim_word_impl, word_depends_on_variable};
use super::{RegulatorSym, TransformOptions, TransformPair, TransformResult};
use crate::algebra::algebraic_letters::join_algebraic_letter_session;
use crate::algebra::linear_factors::{LinearFactorOptions, linear_factors_with_options};
use crate::core::{DigestBuckets, PolyCtx, Rat, structural_bucket_digest};
use crate::error::{Error, Result};
use crate::reduce::MzvReductionTable;
use crate::symbols::{Word, Wordlist, WordlistTerm};

const WORD_ROW_BUCKET_DOMAIN: u64 = 0x5452_574f_5244_0001;
const TRANSFORM_CACHE_BUCKET_DOMAIN: u64 = 0x5452_4341_4348_0001;

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
    word_indices: DigestBuckets,
    terms: Vec<WordlistTerm>,
}

struct ResultBucketDigests {
    regulator: u64,
    word: u64,
}

fn bump_result(
    rows: &mut Vec<ResultRow>,
    row_indices: &mut DigestBuckets,
    regulator: &RegulatorSym,
    word: Word,
    coefficient: Rat,
) -> Result<()> {
    let canonical_regulator = canonicalize_regulator_sym(regulator)?;
    let regulator_digest = regulator_sym_bucket_digest(&canonical_regulator);
    let word_digest = structural_bucket_digest(WORD_ROW_BUCKET_DOMAIN, &word);
    bump_canonical_result(
        rows,
        row_indices,
        &canonical_regulator,
        word,
        coefficient,
        ResultBucketDigests {
            regulator: regulator_digest,
            word: word_digest,
        },
    )
}

fn bump_canonical_result(
    rows: &mut Vec<ResultRow>,
    row_indices: &mut DigestBuckets,
    canonical_regulator: &RegulatorSym,
    word: Word,
    coefficient: Rat,
    digests: ResultBucketDigests,
) -> Result<()> {
    let row_index = if let Some(index) = row_indices.find(digests.regulator, |index| {
        rows.get(index).is_some_and(|row| {
            regulator_sym_structurally_equal(&row.regulator, canonical_regulator)
        })
    }) {
        index
    } else {
        let index = rows.len();
        row_indices.insert(digests.regulator, index);
        rows.push(ResultRow {
            regulator: canonical_regulator.clone(),
            word_indices: DigestBuckets::default(),
            terms: Vec::new(),
        });
        index
    };

    let row = &mut rows[row_index];
    if let Some(index) = row.word_indices.find(digests.word, |index| {
        row.terms.get(index).is_some_and(|term| term.word == word)
    }) {
        row.terms[index].coef = row.terms[index].coef.try_add(&coefficient)?;
    } else {
        row.word_indices.insert(digests.word, row.terms.len());
        row.terms.push(WordlistTerm::new(coefficient, word));
    }
    Ok(())
}

fn finish_result_rows(rows: Vec<ResultRow>) -> TransformResult {
    rows.into_iter()
        .filter_map(|mut row| {
            row.terms.retain(|term| !term.coef.is_zero());
            (!row.terms.is_empty()).then_some(TransformPair {
                shuffle: Wordlist::from(row.terms),
                regulator: row.regulator,
            })
        })
        .collect()
}

#[cfg(test)]
pub(super) fn collect_result_rows_with_forced_collision(
    entries: &[(RegulatorSym, Word, Rat)],
) -> Result<TransformResult> {
    let mut rows = Vec::new();
    let mut row_indices = DigestBuckets::default();
    for (regulator, word, coefficient) in entries {
        bump_canonical_result(
            &mut rows,
            &mut row_indices,
            &canonicalize_regulator_sym(regulator)?,
            word.clone(),
            coefficient.clone(),
            ResultBucketDigests {
                regulator: 0,
                word: 0,
            },
        )?;
    }
    Ok(finish_result_rows(rows))
}

#[derive(Clone)]
struct SignedLinearFactor {
    multiplicity: i64,
    pole: Rat,
}

#[derive(Clone)]
struct TransformCacheEntry {
    variable: usize,
    word: Word,
    value: TransformResult,
}

#[derive(Default)]
pub(super) struct TransformCache {
    buckets: DigestBuckets,
    entries: Vec<TransformCacheEntry>,
}

impl TransformCache {
    fn digest(word: &Word, variable: usize) -> u64 {
        structural_bucket_digest(TRANSFORM_CACHE_BUCKET_DOMAIN, &(variable, word))
    }

    fn get(&self, word: &Word, variable: usize) -> Option<&TransformResult> {
        self.get_in_bucket(word, variable, Self::digest(word, variable))
    }

    pub(super) fn get_in_bucket(
        &self,
        word: &Word,
        variable: usize,
        digest: u64,
    ) -> Option<&TransformResult> {
        self.buckets
            .find(digest, |index| {
                self.entries
                    .get(index)
                    .is_some_and(|entry| entry.variable == variable && entry.word == *word)
            })
            .map(|index| &self.entries[index].value)
    }

    fn insert(&mut self, word: &Word, variable: usize, value: TransformResult) {
        self.insert_in_bucket(word, variable, value, Self::digest(word, variable));
    }

    pub(super) fn insert_in_bucket(
        &mut self,
        word: &Word,
        variable: usize,
        value: TransformResult,
        digest: u64,
    ) {
        if let Some(index) = self.buckets.find(digest, |index| {
            self.entries
                .get(index)
                .is_some_and(|entry| entry.variable == variable && entry.word == *word)
        }) {
            self.entries[index].value = value;
            return;
        }
        let index = self.entries.len();
        self.entries.push(TransformCacheEntry {
            variable,
            word: word.clone(),
            value,
        });
        self.buckets.insert(digest, index);
    }
}

/// Transform one word into shuffle factors and regularized limits.
pub fn transform_word(ctx: &Arc<PolyCtx>, word: &Word, variable: usize) -> Result<TransformResult> {
    transform_word_with_options(ctx, word, variable, &TransformOptions::default())
}

/// Transform one word with an explicit algebraic-letter policy.
pub fn transform_word_with_options(
    ctx: &Arc<PolyCtx>,
    word: &Word,
    variable: usize,
    options: &TransformOptions<'_>,
) -> Result<TransformResult> {
    transform_word_with_options_and_table(ctx, word, variable, options, None)
}

pub(crate) fn transform_word_with_options_and_table(
    ctx: &Arc<PolyCtx>,
    word: &Word,
    variable: usize,
    options: &TransformOptions<'_>,
    table: Option<&MzvReductionTable>,
) -> Result<TransformResult> {
    if variable >= ctx.len() {
        return Err(Error::UnknownVariable(variable.to_string()));
    }
    require_word_context(word, ctx)?;
    let _algebraic_session = options
        .introduce_algebraic_letters
        .then(join_algebraic_letter_session)
        .transpose()?;
    transform_word_impl(
        ctx,
        word,
        variable,
        options,
        table,
        &mut TransformCache::default(),
    )
}

pub(super) fn transform_word_impl(
    ctx: &Arc<PolyCtx>,
    word: &Word,
    variable: usize,
    options: &TransformOptions<'_>,
    table: Option<&MzvReductionTable>,
    cache: &mut TransformCache,
) -> Result<TransformResult> {
    if let Some(cached) = cache.get(word, variable) {
        return Ok(cached.clone());
    }
    if word.is_empty() {
        let output = identity_transform(ctx);
        cache.insert(word, variable, output.clone());
        return Ok(output);
    }

    let limit = reglim_word_impl(ctx, word, variable, table)?;
    let has_variable = word_depends_on_variable(word, variable)?;
    let mut rows = Vec::<ResultRow>::new();
    let mut row_indices = DigestBuckets::default();
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
        cache.insert(word, variable, output.clone());
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

        let factor_options = LinearFactorOptions {
            introduce_algebraic_letters: options.introduce_algebraic_letters,
            forbidden_variables: options.forbidden_variables,
        };
        let numerator =
            linear_factors_with_options(difference.numerator(), variable, &factor_options)?;
        let denominator =
            linear_factors_with_options(difference.denominator(), variable, &factor_options)?;
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
                let transformed =
                    transform_word_impl(ctx, &subword, variable, options, table, cache)?;
                append_factored_rows(&mut rows, &mut row_indices, &transformed, &factors, 1, ctx)?;
            }
        }

        let mut letters = Vec::with_capacity(word.len() - 1);
        letters.extend(word.letters[..index].iter().cloned());
        letters.extend(word.letters[index + 1..].iter().cloned());
        let subword = Word::from(letters);
        if subword.is_empty() || !trailing_zero(&subword) {
            let transformed = transform_word_impl(ctx, &subword, variable, options, table, cache)?;
            append_factored_rows(&mut rows, &mut row_indices, &transformed, &factors, -1, ctx)?;
        }
    }

    let output = finish_result_rows(rows);
    cache.insert(word, variable, output.clone());
    Ok(output)
}

fn append_factored_rows(
    rows: &mut Vec<ResultRow>,
    row_indices: &mut DigestBuckets,
    transformed: &TransformResult,
    factors: &[SignedLinearFactor],
    sign: i64,
    ctx: &Arc<PolyCtx>,
) -> Result<()> {
    for pair in transformed {
        // The regulator does not depend on the appended letter. Canonicalize
        // and hash it once, and clone it only when a new result row is needed.
        let regulator = canonicalize_regulator_sym(&pair.regulator)?;
        let regulator_digest = regulator_sym_bucket_digest(&regulator);
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
                let word = Word::from(letters);
                let word_digest = structural_bucket_digest(WORD_ROW_BUCKET_DOMAIN, &word);
                bump_canonical_result(
                    rows,
                    row_indices,
                    &regulator,
                    word,
                    coefficient,
                    ResultBucketDigests {
                        regulator: regulator_digest,
                        word: word_digest,
                    },
                )?;
            }
        }
    }
    Ok(())
}
