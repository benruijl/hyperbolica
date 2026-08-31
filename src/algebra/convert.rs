//! Endpoint and interval conversions for hyperlogarithm wordlists.

use crate::algebra::shuffle::collect_words;
use crate::core::Rat;
use crate::error::Result;
use crate::symbols::{Letter, Word, Wordlist, WordlistTerm};

fn prepend_letter(wordlist: &Wordlist, letter: &Letter) -> Wordlist {
    Wordlist::new(
        wordlist
            .terms
            .iter()
            .map(|term| {
                let mut letters = Vec::with_capacity(term.word.len() + 1);
                letters.push(letter.clone());
                letters.extend(term.word.letters.iter().cloned());
                WordlistTerm::new(term.coef.clone(), Word::new(letters))
            })
            .collect(),
    )
}

fn negate_coefficients(wordlist: &Wordlist) -> Wordlist {
    Wordlist::new(
        wordlist
            .terms
            .iter()
            .map(|term| WordlistTerm::new(term.coef.negated(), term.word.clone()))
            .collect(),
    )
}

fn union(mut left: Wordlist, right: Wordlist) -> Wordlist {
    left.terms.extend(right.terms);
    left
}

/// Convert the `[0, infinity)` alphabet to `[0, 1]` via `z -> z/(1+z)`.
pub fn convert_zero_one(wordlist: &Wordlist) -> Result<Wordlist> {
    let Some(first) = wordlist.terms.first() else {
        return Ok(wordlist.clone());
    };
    let ctx = first.coef.ctx().clone();
    let zero = Rat::zero(ctx.clone());
    let one = Rat::one(ctx);
    let mut output = Wordlist::default();

    for term in &wordlist.terms {
        let mut partial =
            Wordlist::new(vec![WordlistTerm::new(term.coef.clone(), Word::default())]);
        for letter in &term.word.letters {
            if letter.to_string() == "-1" {
                partial = prepend_letter(&partial, &zero);
            } else {
                let transformed = one.try_div(&one.try_add(letter)?)?;
                partial = union(
                    prepend_letter(&partial, &zero),
                    negate_coefficients(&prepend_letter(&partial, &transformed)),
                );
            }
        }
        output.terms.extend(partial.terms);
    }
    Ok(collect_words(&output))
}

/// Convert `[1, infinity)` to `[0, 1]` via reciprocal substitution.
pub fn convert_one_infinity_to_zero_one(wordlist: &Wordlist) -> Result<Wordlist> {
    let Some(first) = wordlist.terms.first() else {
        return Ok(wordlist.clone());
    };
    let ctx = first.coef.ctx().clone();
    let zero = Rat::zero(ctx.clone());
    let one = Rat::one(ctx);
    let mut output = Wordlist::default();

    for term in &wordlist.terms {
        let mut partial =
            Wordlist::new(vec![WordlistTerm::new(term.coef.clone(), Word::default())]);
        for letter in &term.word.letters {
            if letter.is_zero() {
                partial = prepend_letter(&partial, &zero);
            } else {
                let transformed = one.try_div(letter)?;
                partial = union(
                    prepend_letter(&partial, &zero),
                    negate_coefficients(&prepend_letter(&partial, &transformed)),
                );
            }
        }
        output.terms.extend(partial.terms);
    }
    Ok(collect_words(&output))
}

/// Convert an integral over `[a,b]` to `[0,infinity)`.
pub fn convert_ab_to_zero_infinity(wordlist: &Wordlist, a: &Rat, b: &Rat) -> Result<Wordlist> {
    let minus_one = Rat::from_int(a.ctx().clone(), -1);
    let mut output = Wordlist::default();

    for term in &wordlist.terms {
        let mut partial =
            Wordlist::new(vec![WordlistTerm::new(term.coef.clone(), Word::default())]);
        for letter in &term.word.letters {
            let b_minus_letter = b.try_sub(letter)?;
            let is_upper_endpoint = b_minus_letter.is_zero();
            let mut next = Wordlist::default();
            for entry in partial.terms {
                let mut first_word = entry.word.clone();
                first_word.letters.push(minus_one.clone());
                next.terms
                    .push(WordlistTerm::new(entry.coef.negated(), first_word));
                if !is_upper_endpoint {
                    let transformed = letter.try_sub(a)?.try_div(&b_minus_letter)?;
                    let mut second_word = entry.word;
                    second_word.letters.push(transformed);
                    next.terms.push(WordlistTerm::new(entry.coef, second_word));
                }
            }
            partial = next;
        }
        output.terms.extend(partial.terms);
    }
    Ok(collect_words(&output))
}

#[cfg(test)]
mod tests {
    use crate::core::PolyCtx;

    use super::*;

    #[test]
    fn upper_endpoint_conversion_splits_nonzero_letters() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let input = Wordlist::new(vec![WordlistTerm::new(
            Rat::one(ctx.clone()),
            Word::new(vec![Rat::parse(ctx.clone(), "x").unwrap()]),
        )]);
        let output = convert_one_infinity_to_zero_one(&input).unwrap();
        assert_eq!(output.len(), 2);
        assert_eq!(output.terms[0].word[0], Rat::zero(ctx.clone()));
        assert_eq!(output.terms[1].word[0], Rat::parse(ctx, "1/x").unwrap());
        assert_eq!(output.terms[1].coef.to_string(), "-1");
    }

    #[test]
    fn finite_interval_upper_letter_has_one_branch() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let a = Rat::zero(ctx.clone());
        let b = Rat::one(ctx.clone());
        let input = Wordlist::new(vec![WordlistTerm::new(
            Rat::one(ctx),
            Word::new(vec![b.clone()]),
        )]);
        let output = convert_ab_to_zero_infinity(&input, &a, &b).unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(output.terms[0].word[0].to_string(), "-1");
        assert_eq!(output.terms[0].coef.to_string(), "-1");
    }
}
