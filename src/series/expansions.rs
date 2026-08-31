use std::sync::Arc;

use crate::core::{PolyCtx, Rat};
use crate::error::Result;
use crate::symbols::Word;

/// `table[log_power][power]` for a word-level expansion.
pub type SeriesTable = Vec<Vec<Rat>>;

#[derive(Clone, Copy)]
enum ExpansionPoint {
    Zero,
    Infinity,
}

pub fn expand_zero_word(word: &Word, minimum_order: i64) -> Result<SeriesTable> {
    if word.is_empty() || minimum_order < 0 {
        return Ok(Vec::new());
    }
    expand_common(
        word,
        minimum_order,
        ExpansionPoint::Zero,
        word[0].ctx().clone(),
    )
}

pub fn expand_infinity_word(word: &Word, minimum_order: i64) -> Result<SeriesTable> {
    if word.is_empty() || minimum_order < 0 {
        return Ok(Vec::new());
    }
    expand_common(
        word,
        minimum_order,
        ExpansionPoint::Infinity,
        word[0].ctx().clone(),
    )
}

pub fn expand_zero_word_in_context(
    ctx: Arc<PolyCtx>,
    word: &Word,
    minimum_order: i64,
) -> Result<SeriesTable> {
    if minimum_order < 0 {
        return Ok(Vec::new());
    }
    expand_common(word, minimum_order, ExpansionPoint::Zero, ctx)
}

pub fn expand_infinity_word_in_context(
    ctx: Arc<PolyCtx>,
    word: &Word,
    minimum_order: i64,
) -> Result<SeriesTable> {
    if minimum_order < 0 {
        return Ok(Vec::new());
    }
    expand_common(word, minimum_order, ExpansionPoint::Infinity, ctx)
}

fn zero_table(ctx: &Arc<PolyCtx>, rows: usize, columns: usize) -> SeriesTable {
    (0..rows)
        .map(|_| vec![Rat::zero(ctx.clone()); columns])
        .collect()
}

fn expand_common(
    word: &Word,
    minimum_order: i64,
    point: ExpansionPoint,
    ctx: Arc<PolyCtx>,
) -> Result<SeriesTable> {
    if word.is_empty() {
        return Ok(vec![vec![Rat::one(ctx)]]);
    }

    let tail = Word::new(word.letters[1..].to_vec());
    let subseries = expand_common(&tail, minimum_order, point, ctx.clone())?;
    if subseries.is_empty() {
        return Ok(Vec::new());
    }

    let maximum_log_power = subseries.len();
    let mut result = zero_table(&ctx, maximum_log_power + 1, (minimum_order + 1) as usize);
    let mut maximum_powers = vec![-1_i64; maximum_log_power + 1];
    let first_letter = &word[0];
    let first_letter_is_zero = first_letter.is_zero();

    for (log_power, row) in subseries.iter().enumerate() {
        let bump_log =
            !row.is_empty() && (matches!(point, ExpansionPoint::Infinity) || first_letter_is_zero);
        if bump_log {
            let signed = if matches!(point, ExpansionPoint::Zero) {
                row[0].clone()
            } else {
                row[0].negated()
            };
            result[log_power + 1][0] = result[log_power + 1][0].try_add(&signed)?;
            maximum_powers[log_power + 1] = 0;
        }

        for power in 0..minimum_order {
            let mut coefficient = Rat::zero(ctx.clone());
            match point {
                ExpansionPoint::Zero if !first_letter_is_zero => {
                    let upper = power.min(row.len() as i64 - 1);
                    for inner in 0..=upper {
                        let divisor = first_letter.pow(power - inner + 1)?;
                        coefficient =
                            coefficient.try_sub(&row[inner as usize].try_div(&divisor)?)?;
                    }
                }
                ExpansionPoint::Zero => {
                    if row.len() as i64 > power + 1 {
                        coefficient = row[(power + 1) as usize].clone();
                    }
                }
                ExpansionPoint::Infinity => {
                    if row.len() as i64 > power + 1 {
                        coefficient = row[(power + 1) as usize].negated();
                    }
                    if !first_letter_is_zero {
                        let upper = power.min(row.len() as i64 - 1);
                        for inner in 0..=upper {
                            let product = row[inner as usize]
                                .try_mul(&first_letter.pow(power - inner + 1)?)?;
                            coefficient = coefficient.try_sub(&product)?;
                        }
                    }
                }
            }

            if coefficient.is_zero() {
                continue;
            }
            coefficient = coefficient.negated();
            for lower_log_power in (0..=log_power).rev() {
                coefficient = coefficient
                    .negated()
                    .try_div(&Rat::from_int(ctx.clone(), power + 1))?;
                result[lower_log_power][(power + 1) as usize] =
                    result[lower_log_power][(power + 1) as usize].try_add(&coefficient)?;
                maximum_powers[lower_log_power] = maximum_powers[lower_log_power].max(power + 1);
            }
        }
    }

    for (row, maximum) in result.iter_mut().zip(maximum_powers) {
        if maximum < 0 {
            row.clear();
        } else {
            row.truncate((maximum + 1) as usize);
        }
    }
    while result.last().is_some_and(Vec::is_empty) {
        result.pop();
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{PolyCtx, Rat};

    #[test]
    fn empty_word_is_the_series_unit_with_context() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        assert_eq!(
            expand_zero_word_in_context(ctx.clone(), &Word::default(), 3).unwrap(),
            vec![vec![Rat::one(ctx)]]
        );
    }

    #[test]
    fn zero_letter_expands_to_a_logarithm() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let word = Word::new(vec![Rat::zero(ctx.clone())]);
        let table = expand_zero_word(&word, 2).unwrap();
        assert_eq!(table.len(), 2);
        assert!(table[0].is_empty());
        assert_eq!(table[1], vec![Rat::one(ctx)]);
    }
}
