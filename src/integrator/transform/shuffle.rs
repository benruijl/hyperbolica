use std::sync::Arc;

use super::collection::{require_word_context, shuffle_symbolic_sym};
use super::word::{TransformCache, identity_transform, transform_word_impl};
use super::{RegTermSym, RegulatorSym, TransformOptions, TransformPair, TransformResult};
use crate::algebra::AlgebraicLetterSession;
use crate::algebra::algebraic_letters::join_algebraic_letter_session;
use crate::algebra::shuffle::shuffle_product;
use crate::core::{DigestBuckets, PolyCtx, Rat, structural_bucket_digest};
use crate::error::{Error, Result};
use crate::reduce::MzvReductionTable;
use crate::symbols::Word;

const LOG_LETTER_BUCKET_DOMAIN: u64 = 0x5452_4c4f_474c_0001;

fn is_log_power(word: &Word) -> bool {
    let Some(first) = word.letters.first() else {
        return false;
    };
    word.letters.iter().all(|letter| letter.equal(first))
}

fn factorial_rat(ctx: &Arc<PolyCtx>, n: usize) -> Result<Rat> {
    let mut value = Rat::one(ctx.clone());
    for factor in 2..=n {
        let factor = i64::try_from(factor)
            .map_err(|_| Error::InvalidInput("word length does not fit in i64".into()))?;
        value = value.try_mul(&Rat::from_int(ctx.clone(), factor))?;
    }
    Ok(value)
}

fn scale_regulator(regulator: &RegulatorSym, scalar: &Rat) -> Result<RegulatorSym> {
    regulator
        .iter()
        .map(|term| {
            Ok(RegTermSym {
                coef: term.coef.try_mul_rat(scalar)?,
                key: term.key.clone(),
            })
        })
        .collect()
}

pub(super) struct LogPowerGrouping {
    pub(super) combined: Vec<Word>,
    pub(super) groups: Vec<(Rat, usize)>,
    pub(super) combinatorial_factor: Rat,
    pub(super) repeated: bool,
}

pub(super) fn group_log_powers_with_digest(
    ctx: &Arc<PolyCtx>,
    words: &[Word],
    mut digest_letter: impl FnMut(&Rat) -> u64,
) -> Result<LogPowerGrouping> {
    let mut buckets = DigestBuckets::default();
    let mut groups = Vec::<(Rat, usize)>::new();
    let mut combined = Vec::<Word>::new();
    let mut combinatorial_factor = Rat::one(ctx.clone());
    let mut repeated = false;

    for word in words {
        if word.is_empty() {
            continue;
        }
        if is_log_power(word) {
            combinatorial_factor =
                combinatorial_factor.try_div(&factorial_rat(ctx, word.len())?)?;
            let letter = &word[0];
            let digest = digest_letter(letter);
            if let Some(index) = buckets.find(digest, |index| {
                groups
                    .get(index)
                    .is_some_and(|(candidate, _)| candidate == letter)
            }) {
                groups[index].1 = groups[index].1.checked_add(word.len()).ok_or_else(|| {
                    Error::InvalidInput("combined logarithm depth overflowed usize".into())
                })?;
                repeated = true;
            } else {
                buckets.insert(digest, groups.len());
                groups.push((letter.clone(), word.len()));
            }
        } else {
            combined.push(word.clone());
        }
    }

    Ok(LogPowerGrouping {
        combined,
        groups,
        combinatorial_factor,
        repeated,
    })
}

/// Transform a shuffle product of words.
pub fn transform_shuffle(
    ctx: &Arc<PolyCtx>,
    words: &[Word],
    variable: usize,
) -> Result<TransformResult> {
    transform_shuffle_with_options(ctx, words, variable, &TransformOptions::default())
}

/// Transform a shuffle product with an explicit algebraic-letter policy.
pub fn transform_shuffle_with_options(
    ctx: &Arc<PolyCtx>,
    words: &[Word],
    variable: usize,
    options: &TransformOptions<'_>,
) -> Result<TransformResult> {
    TransformSession::new(ctx, variable, *options, None)?.transform(words)
}

pub(crate) fn transform_shuffle_with_options_and_table(
    ctx: &Arc<PolyCtx>,
    words: &[Word],
    variable: usize,
    options: &TransformOptions<'_>,
    table: &MzvReductionTable,
) -> Result<TransformResult> {
    TransformSession::new(ctx, variable, *options, Some(table))?.transform(words)
}

/// Own a word/subword cache for a fixed integration context and policy.
/// Different shuffle spines within one step commonly contain the same words;
/// retaining their recursive transforms avoids repeating factorization and
/// endpoint regularization. The session cannot be reused with another table,
/// variable, or algebraic-letter policy, and is dropped at the step boundary.
pub(crate) struct TransformSession<'a> {
    ctx: &'a Arc<PolyCtx>,
    variable: usize,
    options: TransformOptions<'a>,
    table: Option<&'a MzvReductionTable>,
    cache: TransformCache,
    // Cached Wm/Wp indices must remain associated with their quadratics.
    _algebraic_session: Option<AlgebraicLetterSession>,
}

impl<'a> TransformSession<'a> {
    pub(crate) fn new(
        ctx: &'a Arc<PolyCtx>,
        variable: usize,
        options: TransformOptions<'a>,
        table: Option<&'a MzvReductionTable>,
    ) -> Result<Self> {
        if variable >= ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        Ok(Self {
            ctx,
            variable,
            options,
            table,
            cache: TransformCache::default(),
            _algebraic_session: options
                .introduce_algebraic_letters
                .then(join_algebraic_letter_session)
                .transpose()?,
        })
    }

    pub(crate) fn transform(&mut self, words: &[Word]) -> Result<TransformResult> {
        for word in words {
            require_word_context(word, self.ctx)?;
        }
        transform_shuffle_impl(
            self.ctx,
            words,
            self.variable,
            &self.options,
            self.table,
            &mut self.cache,
        )
    }
}

fn transform_shuffle_impl(
    ctx: &Arc<PolyCtx>,
    words: &[Word],
    variable: usize,
    options: &TransformOptions<'_>,
    table: Option<&MzvReductionTable>,
    cache: &mut TransformCache,
) -> Result<TransformResult> {
    if words.is_empty() {
        return Ok(identity_transform(ctx));
    }

    let LogPowerGrouping {
        mut combined,
        groups: log_groups,
        mut combinatorial_factor,
        repeated,
    } = group_log_powers_with_digest(ctx, words, |letter| {
        structural_bucket_digest(LOG_LETTER_BUCKET_DOMAIN, letter)
    })?;

    if repeated {
        for (letter, count) in log_groups {
            combined.push(Word::from(vec![letter; count]));
            combinatorial_factor = combinatorial_factor.try_mul(&factorial_rat(ctx, count)?)?;
        }
        let transformed = transform_shuffle_impl(ctx, &combined, variable, options, table, cache)?;
        return transformed
            .into_iter()
            .map(|pair| {
                Ok(TransformPair {
                    shuffle: pair.shuffle,
                    regulator: scale_regulator(&pair.regulator, &combinatorial_factor)?,
                })
            })
            .collect();
    }

    let mut accumulator = identity_transform(ctx);
    for word in words {
        let transformed = transform_word_impl(ctx, word, variable, options, table, cache)?;
        if transformed.is_empty() {
            return Ok(Vec::new());
        }
        let mut next = Vec::with_capacity(accumulator.len().saturating_mul(transformed.len()));
        for left in &accumulator {
            for right in &transformed {
                next.push(TransformPair {
                    shuffle: shuffle_product(&left.shuffle, &right.shuffle),
                    regulator: shuffle_symbolic_sym(&left.regulator, &right.regulator)?,
                });
            }
        }
        accumulator = next;
    }
    Ok(accumulator)
}
