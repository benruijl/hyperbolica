use std::sync::Arc;

use crate::algebra::shuffle::collect_words;
use crate::core::{Poly, PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::symbols::{Word, Wordlist, WordlistTerm};

fn require_context(value: &Rat, ctx: &PolyCtx) -> Result<()> {
    if value.ctx().vars() == ctx.vars() {
        Ok(())
    } else {
        Err(Error::ContextMismatch)
    }
}

/// Differentiate a rational linear combination of hyperlogarithm words.
///
/// The anchor is the selected polynomial variable. For a non-empty word the
/// first letter contributes the usual `coef / (x - letter)` chain-rule term.
pub fn differentiate_wordlist(wordlist: &Wordlist, variable: usize) -> Result<Wordlist> {
    let Some(first) = wordlist.terms.first() else {
        return Ok(Wordlist::default());
    };
    let ctx: Arc<PolyCtx> = first.coef.ctx().clone();
    let variable_rat = Rat::from_poly(Poly::generator(ctx.clone(), variable)?);
    let mut output = Wordlist::default();

    for term in &wordlist.terms {
        require_context(&term.coef, &ctx)?;
        for letter in &term.word.letters {
            require_context(letter, &ctx)?;
        }

        let coefficient_derivative = term.coef.derivative(variable)?;
        if !coefficient_derivative.is_zero() {
            output
                .terms
                .push(WordlistTerm::new(coefficient_derivative, term.word.clone()));
        }

        let Some(first_letter) = term.word.letters.first() else {
            continue;
        };
        let denominator = variable_rat.try_sub(first_letter)?;
        if denominator.is_zero() {
            // This pathological Hlog(x, [x, ...]) is undefined upstream as
            // well. Keeping the coefficient-derivative piece is the useful,
            // deterministic behavior of HyperFLINT's guard.
            continue;
        }
        output.terms.push(WordlistTerm::new(
            term.coef.try_div(&denominator)?,
            Word::from(term.word.letters[1..].to_vec()),
        ));
    }
    Ok(collect_words(&output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn differentiates_coefficients_and_the_hyperlog_chain() {
        let ctx = PolyCtx::new(["x", "a"]).unwrap();
        let input = Wordlist::new(vec![WordlistTerm::new(
            Rat::parse(ctx.clone(), "x^2").unwrap(),
            Word::new(vec![Rat::parse(ctx.clone(), "a").unwrap()]),
        )]);
        let output = differentiate_wordlist(&input, 0).unwrap();

        assert_eq!(output.terms.len(), 2);
        assert_eq!(
            output.terms[0].coef,
            Rat::parse(ctx.clone(), "2*x").unwrap()
        );
        assert_eq!(output.terms[0].word, input.terms[0].word);
        assert_eq!(output.terms[1].coef, Rat::parse(ctx, "x^2/(x-a)").unwrap());
        assert!(output.terms[1].word.is_empty());
    }

    #[test]
    fn empty_words_only_differentiate_their_coefficient() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let input = Wordlist::new(vec![WordlistTerm::new(
            Rat::parse(ctx.clone(), "x^3+2").unwrap(),
            Word::default(),
        )]);
        let output = differentiate_wordlist(&input, 0).unwrap();
        assert_eq!(output.terms[0].coef, Rat::parse(ctx, "3*x^2").unwrap());
        assert!(output.terms[0].word.is_empty());
    }
}
