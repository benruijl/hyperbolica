use std::sync::Arc;

use crate::algebra::shuffle::{collect_words, concat_mul, shuffle_product};
use crate::core::{PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::symbols::{Letter, Word, Wordlist, WordlistTerm};

fn require_rat_context(value: &Rat, ctx: &PolyCtx) -> Result<()> {
    if value.ctx().vars() == ctx.vars() {
        Ok(())
    } else {
        Err(Error::ContextMismatch)
    }
}

fn require_word_context(word: &Word, ctx: &PolyCtx) -> Result<()> {
    for letter in &word.letters {
        require_rat_context(letter, ctx)?;
    }
    Ok(())
}

fn factorial_rat(ctx: Arc<PolyCtx>, n: usize) -> Result<Rat> {
    let mut factorial = Rat::one(ctx.clone());
    for value in 2..=n {
        let value = i64::try_from(value)
            .map_err(|_| Error::InvalidInput("word length does not fit in i64".into()))?;
        factorial = factorial.try_mul(&Rat::from_int(ctx.clone(), value))?;
    }
    Ok(factorial)
}

fn signed_unit(ctx: Arc<PolyCtx>, exponent: usize) -> Rat {
    Rat::from_int(ctx, if exponent.is_multiple_of(2) { 1 } else { -1 })
}

/// Shuffle-regularize the trailing zero letters of one word.
///
/// The context-free overload follows HyperFLINT's historical convention for
/// the empty word and returns an empty wordlist, because there is no ambient
/// ring from which to construct the coefficient one. Use
/// [`regzero_word_in_ctx`] when the empty-word identity is required.
pub fn regzero_word(word: &Word) -> Result<Wordlist> {
    let Some(first) = word.letters.first() else {
        return Ok(Wordlist::default());
    };
    regzero_word_in_ctx(first.ctx(), word)
}

/// Context-seeded form of [`regzero_word`].
pub fn regzero_word_in_ctx(ctx: &Arc<PolyCtx>, word: &Word) -> Result<Wordlist> {
    require_word_context(word, ctx)?;
    if word.is_empty() {
        return Ok(Wordlist::from(vec![WordlistTerm::new(
            Rat::one(ctx.clone()),
            Word::default(),
        )]));
    }

    let trailing_zeros = word
        .letters
        .iter()
        .rev()
        .take_while(|letter| letter.is_zero())
        .count();
    if trailing_zeros == word.len() {
        return Ok(Wordlist::default());
    }
    if trailing_zeros == 0 {
        return Ok(Wordlist::from(vec![WordlistTerm::new(
            Rat::one(ctx.clone()),
            word.clone(),
        )]));
    }

    let anchor_index = word.len() - trailing_zeros - 1;
    let zeros = Word::from(vec![Rat::zero(ctx.clone()); trailing_zeros]);
    let prefix = Word::from(word.letters[..anchor_index].to_vec());
    let zeros_wordlist = Wordlist::from(vec![WordlistTerm::new(
        signed_unit(ctx.clone(), trailing_zeros),
        zeros,
    )]);
    let prefix_wordlist = Wordlist::from(vec![WordlistTerm::new(Rat::one(ctx.clone()), prefix)]);
    let shuffled = shuffle_product(&zeros_wordlist, &prefix_wordlist);
    let anchor = word[anchor_index].clone();

    Ok(Wordlist::from(
        shuffled
            .terms
            .into_iter()
            .map(|term| {
                let mut letters = term.word.letters;
                letters.push(anchor.clone());
                WordlistTerm::new(term.coef, Word::from(letters))
            })
            .collect::<Vec<_>>(),
    ))
}

/// Apply [`regzero_word_in_ctx`] termwise and collect equal words.
pub fn reg0(wordlist: &Wordlist) -> Result<Wordlist> {
    let Some(first) = wordlist.terms.first() else {
        return Ok(Wordlist::default());
    };
    let ctx = first.coef.ctx().clone();
    let mut result = Wordlist::default();
    for term in &wordlist.terms {
        require_rat_context(&term.coef, &ctx)?;
        require_word_context(&term.word, &ctx)?;
        let regularized = regzero_word_in_ctx(&ctx, &term.word)?;
        for subterm in regularized.terms {
            result.terms.push(WordlistTerm::new(
                subterm.coef.try_mul(&term.coef)?,
                subterm.word,
            ));
        }
    }
    Ok(collect_words(&result))
}

#[derive(Clone, Copy)]
enum RegSide {
    Head,
    Tail,
}

#[allow(clippy::too_many_arguments)]
fn emit_partial_regularization(
    output: &mut Wordlist,
    side: RegSide,
    letter: &Letter,
    substitute: &Letter,
    run_length: usize,
    shuffle_length: usize,
    anchor: &Letter,
    term_coefficient: &Rat,
    stripped: &Word,
    ctx: &Arc<PolyCtx>,
) -> Result<()> {
    let prefactor = if shuffle_length == run_length {
        Rat::one(ctx.clone())
    } else {
        substitute
            .pow(i64::try_from(run_length - shuffle_length).map_err(|_| {
                Error::InvalidInput("regularization exponent does not fit in i64".into())
            })?)?
            .try_div(&factorial_rat(ctx.clone(), run_length - shuffle_length)?)?
    };

    let repeated = Word::from(vec![letter.clone(); shuffle_length]);
    let left = Wordlist::from(vec![WordlistTerm::new(
        signed_unit(ctx.clone(), shuffle_length),
        repeated,
    )]);
    let right = Wordlist::from(vec![WordlistTerm::new(
        term_coefficient.clone(),
        stripped.clone(),
    )]);
    let shuffled = shuffle_product(&left, &right);
    let anchor_wordlist = Wordlist::from(vec![WordlistTerm::new(
        prefactor,
        Word::from(vec![anchor.clone()]),
    )]);
    let combined = match side {
        RegSide::Head => concat_mul(&anchor_wordlist, &shuffled),
        RegSide::Tail => concat_mul(&shuffled, &anchor_wordlist),
    };
    output.terms.extend(combined.terms);
    Ok(())
}

fn regularize_side(
    wordlist: &Wordlist,
    letter: &Letter,
    substitute: &Letter,
    side: RegSide,
) -> Result<Wordlist> {
    if letter.ctx().vars() != substitute.ctx().vars() {
        return Err(Error::ContextMismatch);
    }
    let ctx = letter.ctx().clone();
    let mut result = Wordlist::default();

    for term in &wordlist.terms {
        require_rat_context(&term.coef, &ctx)?;
        require_word_context(&term.word, &ctx)?;
        if term.word.is_empty() {
            result.terms.push(term.clone());
            continue;
        }

        let run_length = match side {
            RegSide::Head => term
                .word
                .letters
                .iter()
                .take_while(|candidate| candidate.equal(letter))
                .count(),
            RegSide::Tail => term
                .word
                .letters
                .iter()
                .rev()
                .take_while(|candidate| candidate.equal(letter))
                .count(),
        };

        if run_length == term.word.len() {
            let prefactor = substitute
                .pow(i64::try_from(run_length).map_err(|_| {
                    Error::InvalidInput("regularization exponent does not fit in i64".into())
                })?)?
                .try_div(&factorial_rat(ctx.clone(), run_length)?)?;
            result.terms.push(WordlistTerm::new(
                term.coef.try_mul(&prefactor)?,
                Word::default(),
            ));
            continue;
        }

        let (anchor, stripped) = match side {
            RegSide::Head => (
                term.word[run_length].clone(),
                Word::from(term.word.letters[run_length + 1..].to_vec()),
            ),
            RegSide::Tail => {
                let anchor_index = term.word.len() - run_length - 1;
                (
                    term.word[anchor_index].clone(),
                    Word::from(term.word.letters[..anchor_index].to_vec()),
                )
            }
        };

        for shuffle_length in 0..=run_length {
            emit_partial_regularization(
                &mut result,
                side,
                letter,
                substitute,
                run_length,
                shuffle_length,
                &anchor,
                &term.coef,
                &stripped,
                &ctx,
            )?;
        }
    }
    Ok(collect_words(&result))
}

/// Shuffle-regularize a leading run of `letter`, replacing removed powers by
/// `substitute`.
pub fn reg_head(wordlist: &Wordlist, letter: &Letter, substitute: &Letter) -> Result<Wordlist> {
    regularize_side(wordlist, letter, substitute, RegSide::Head)
}

/// Shuffle-regularize a trailing run of `letter`, replacing removed powers by
/// `substitute`.
pub fn reg_tail(wordlist: &Wordlist, letter: &Letter, substitute: &Letter) -> Result<Wordlist> {
    regularize_side(wordlist, letter, substitute, RegSide::Tail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Arc<PolyCtx> {
        PolyCtx::new(["x"]).unwrap()
    }

    fn integer(ctx: &Arc<PolyCtx>, value: i64) -> Rat {
        Rat::from_int(ctx.clone(), value)
    }

    fn word(ctx: &Arc<PolyCtx>, values: &[i64]) -> Word {
        Word::from(
            values
                .iter()
                .map(|value| integer(ctx, *value))
                .collect::<Vec<_>>(),
        )
    }

    fn term(ctx: &Arc<PolyCtx>, coefficient: &str, values: &[i64]) -> WordlistTerm {
        WordlistTerm::new(
            Rat::parse(ctx.clone(), coefficient).unwrap(),
            word(ctx, values),
        )
    }

    #[test]
    fn regzero_matches_upstream_shuffle_regularization() {
        let ctx = context();
        assert_eq!(
            regzero_word_in_ctx(&ctx, &word(&ctx, &[1, 2, 0])).unwrap(),
            Wordlist::from(vec![
                term(&ctx, "-1", &[0, 1, 2]),
                term(&ctx, "-1", &[1, 0, 2]),
            ])
        );
        assert!(
            regzero_word_in_ctx(&ctx, &word(&ctx, &[0, 0]))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            regzero_word_in_ctx(&ctx, &Word::default()).unwrap(),
            Wordlist::from(vec![term(&ctx, "1", &[])])
        );
        assert!(regzero_word(&Word::default()).unwrap().is_empty());
    }

    #[test]
    fn reg0_is_linear_and_collects_cancellation() {
        let ctx = context();
        let input = Wordlist::from(vec![
            term(&ctx, "2", &[1, 0]),
            term(&ctx, "-2", &[1, 0]),
            term(&ctx, "3", &[1]),
        ]);
        assert_eq!(
            reg0(&input).unwrap(),
            Wordlist::from(vec![term(&ctx, "3", &[1])])
        );
    }

    #[test]
    fn head_and_tail_preserve_upstream_order_and_coefficients() {
        let ctx = context();
        let zero = integer(&ctx, 0);
        let three = integer(&ctx, 3);

        assert_eq!(
            reg_head(
                &Wordlist::from(vec![term(&ctx, "1", &[0, 0, 2])]),
                &zero,
                &three,
            )
            .unwrap(),
            Wordlist::from(vec![
                term(&ctx, "9/2", &[2]),
                term(&ctx, "-3", &[2, 0]),
                term(&ctx, "1", &[2, 0, 0]),
            ])
        );
        assert_eq!(
            reg_tail(
                &Wordlist::from(vec![term(&ctx, "1", &[2, 0, 0])]),
                &zero,
                &three,
            )
            .unwrap(),
            Wordlist::from(vec![
                term(&ctx, "9/2", &[2]),
                term(&ctx, "-3", &[0, 2]),
                term(&ctx, "1", &[0, 0, 2]),
            ])
        );
    }

    #[test]
    fn complete_edge_run_collapses_to_substitute_power_over_factorial() {
        let ctx = context();
        let zero = integer(&ctx, 0);
        let three = integer(&ctx, 3);
        let input = Wordlist::from(vec![term(&ctx, "2", &[0, 0])]);
        let expected = Wordlist::from(vec![term(&ctx, "9", &[])]);

        assert_eq!(reg_head(&input, &zero, &three).unwrap(), expected);
        assert_eq!(reg_tail(&input, &zero, &three).unwrap(), expected);
    }
}
