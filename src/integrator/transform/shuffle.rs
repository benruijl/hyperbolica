use std::collections::HashMap;
use std::sync::Arc;

use super::collection::{require_word_context, shuffle_symbolic_sym};
use super::word::{identity_transform, transform_word};
use super::{RegTermSym, RegulatorSym, TransformPair, TransformResult};
use crate::algebra::shuffle::shuffle_product;
use crate::core::{PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::symbols::Word;

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

/// Transform a shuffle product of words.
pub fn transform_shuffle(
    ctx: &Arc<PolyCtx>,
    words: &[Word],
    variable: usize,
) -> Result<TransformResult> {
    if variable >= ctx.len() {
        return Err(Error::UnknownVariable(variable.to_string()));
    }
    for word in words {
        require_word_context(word, ctx)?;
    }
    if words.is_empty() {
        return Ok(identity_transform(ctx));
    }

    let mut letters = HashMap::<String, Rat>::new();
    let mut letter_order = Vec::<String>::new();
    let mut log_counts = HashMap::<String, usize>::new();
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
            let key = word[0].to_string();
            if let Some(count) = log_counts.get_mut(&key) {
                *count = count.checked_add(word.len()).ok_or_else(|| {
                    Error::InvalidInput("combined logarithm depth overflowed usize".into())
                })?;
                repeated = true;
            } else {
                log_counts.insert(key.clone(), word.len());
                letters.insert(key.clone(), word[0].clone());
                letter_order.push(key);
            }
        } else {
            combined.push(word.clone());
        }
    }

    if repeated {
        for key in letter_order {
            let count = log_counts[&key];
            combined.push(Word::from(vec![letters[&key].clone(); count]));
        }
        for count in log_counts.values() {
            combinatorial_factor = combinatorial_factor.try_mul(&factorial_rat(ctx, *count)?)?;
        }
        let transformed = transform_shuffle(ctx, &combined, variable)?;
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
        let transformed = transform_word(ctx, word, variable)?;
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
