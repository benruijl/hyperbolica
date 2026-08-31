//! Conversion of rational/Hlog expression trees into regulators at infinity.
//!
//! The seven Hlog cases intentionally appear in the same order as
//! HyperFLINT's `convert_hlog.cpp`.  In particular, the all-zero case must
//! pre-empt trailing-zero regularization, which in turn must pre-empt the
//! divergent-first-letter check.

use std::sync::Arc;

use crate::algebra::shuffle::{concat_mul, shuffle_product};
use crate::core::{PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::integrator::{RegKey, RegTerm, Regulator, canonicalize_regulator, shuffle_symbolic};
use crate::symbols::{Word, Wordlist, WordlistTerm};

use super::Expr;

/// One expression-valued coefficient produced while stripping a word tail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegTailExprTerm {
    pub coef: Expr,
    pub word: Word,
}

fn conversion_error(message: impl Into<String>) -> Error {
    Error::InvalidInput(format!("convert_to_hlog_reg_inf: {}", message.into()))
}

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

fn factorial(ctx: &Arc<PolyCtx>, n: usize) -> Result<Rat> {
    let mut value = Rat::one(ctx.clone());
    for factor in 2..=n {
        let factor = i64::try_from(factor)
            .map_err(|_| conversion_error("word is too long to represent its factorial exactly"))?;
        value = value.try_mul(&Rat::from_int(ctx.clone(), factor))?;
    }
    Ok(value)
}

fn sign(ctx: &Arc<PolyCtx>, n: usize) -> Rat {
    Rat::from_int(ctx.clone(), if n.is_multiple_of(2) { 1 } else { -1 })
}

fn word_as_regkey(word: Word) -> RegKey {
    vec![word]
}

fn log_power(ctx: &Arc<PolyCtx>, letter: Rat, n: usize) -> Result<Regulator> {
    let coefficient = sign(ctx, n).try_div(&factorial(ctx, n)?)?;
    Ok(vec![RegTerm {
        coef: coefficient,
        key: word_as_regkey(Word::new(vec![letter; n])),
    }])
}

fn case_general(var: &Rat, word: &[Rat], ctx: &Arc<PolyCtx>) -> Result<Regulator> {
    let one = Rat::one(ctx.clone());
    let minus_one = Rat::from_int(ctx.clone(), -1);
    let mut result = Wordlist::new(vec![WordlistTerm::new(one.clone(), Word::default())]);

    for letter in word {
        let mut left = Wordlist::new(vec![WordlistTerm::new(
            one.clone(),
            Word::new(vec![minus_one.clone()]),
        )]);
        if !letter.is_zero() {
            left.terms.push(WordlistTerm::new(
                minus_one.clone(),
                Word::new(vec![var.try_div(letter)?.try_sub(&one)?]),
            ));
        }
        result = concat_mul(&left, &result);
    }

    Ok(result
        .terms
        .into_iter()
        .map(|term| RegTerm {
            coef: term.coef,
            key: word_as_regkey(term.word),
        })
        .collect())
}

fn shuffle_stripped_with_prefix(
    count: usize,
    letter_to_strip: &Rat,
    prefix: &Word,
    ctx: &Arc<PolyCtx>,
) -> Wordlist {
    let left = Wordlist::new(vec![WordlistTerm::new(
        sign(ctx, count),
        Word::new(vec![letter_to_strip.clone(); count]),
    )]);
    let right = Wordlist::new(vec![WordlistTerm::new(
        Rat::one(ctx.clone()),
        prefix.clone(),
    )]);
    shuffle_product(&left, &right)
}

/// Strip a repeated trailing letter while retaining an expression-valued
/// substitute in the coefficients.
///
/// Every word returned by the non-collapse branch ends in a letter unequal to
/// `letter_to_strip`; this is the recursion invariant needed by case 4 of
/// [`convert_to_hlog_reg_inf_hlog`].
pub fn reg_tail_expr(
    word: &Word,
    letter_to_strip: &Rat,
    substitute: &Expr,
    ctx: &Arc<PolyCtx>,
) -> Result<Vec<RegTailExprTerm>> {
    require_rat_context(letter_to_strip, ctx)?;
    require_word_context(word, ctx)?;

    if word.is_empty() {
        return Ok(vec![RegTailExprTerm {
            coef: Expr::leaf(Rat::one(ctx.clone())),
            word: word.clone(),
        }]);
    }

    let trailing = word
        .letters
        .iter()
        .rev()
        .take_while(|letter| letter.equal(letter_to_strip))
        .count();

    if trailing == word.len() {
        let coefficient = Rat::one(ctx.clone()).try_div(&factorial(ctx, trailing)?)?;
        let exponent = i64::try_from(trailing)
            .map_err(|_| conversion_error("word length exceeds the expression exponent range"))?;
        return Ok(vec![RegTailExprTerm {
            coef: Expr::times(vec![
                Expr::leaf(coefficient),
                Expr::power(substitute.clone(), exponent)?,
            ]),
            word: Word::default(),
        }]);
    }

    let anchor_index = word.len() - trailing - 1;
    let prefix = Word::new(word.letters[..anchor_index].to_vec());
    let anchor = word.letters[anchor_index].clone();
    let mut output = Vec::new();

    for stripped_count in 0..=trailing {
        let remaining_power = trailing - stripped_count;
        let prefactor = if remaining_power == 0 {
            Expr::leaf(Rat::one(ctx.clone()))
        } else {
            let exponent = i64::try_from(remaining_power).map_err(|_| {
                conversion_error("word length exceeds the expression exponent range")
            })?;
            Expr::times(vec![
                Expr::leaf(Rat::one(ctx.clone()).try_div(&factorial(ctx, remaining_power)?)?),
                Expr::power(substitute.clone(), exponent)?,
            ])
        };

        for shuffled in
            shuffle_stripped_with_prefix(stripped_count, letter_to_strip, &prefix, ctx).terms
        {
            let mut new_word = shuffled.word;
            new_word.letters.push(anchor.clone());
            output.push(RegTailExprTerm {
                coef: Expr::times(vec![Expr::leaf(shuffled.coef), prefactor.clone()]),
                word: new_word,
            });
        }
    }

    if output.iter().any(|term| {
        term.word.is_empty()
            || term
                .word
                .letters
                .last()
                .is_some_and(|letter| letter.equal(letter_to_strip))
    }) {
        return Err(conversion_error(
            "internal trailing-letter regularization invariant failed",
        ));
    }
    Ok(output)
}

/// Convert one `Hlog[var, word]` using HyperFLINT's ordered seven-case map.
pub fn convert_to_hlog_reg_inf_hlog(
    var: &Rat,
    word: &[Rat],
    ctx: &Arc<PolyCtx>,
) -> Result<Regulator> {
    require_rat_context(var, ctx)?;
    for letter in word {
        require_rat_context(letter, ctx)?;
    }
    let one = Rat::one(ctx.clone());

    // Case 1: the empty iterated integral is the multiplicative identity.
    if word.is_empty() {
        return Ok(vec![RegTerm {
            coef: one,
            key: RegKey::new(),
        }]);
    }

    // Case 2: the base point itself is divergent.
    if var.is_zero() {
        return Ok(Regulator::new());
    }

    // Case 3 must pre-empt both trailing-zero and first-letter checks.
    if word.iter().all(Rat::is_zero) {
        if var.equal(&one) {
            return Ok(Regulator::new());
        }
        return log_power(ctx, var.negated(), word.len());
    }

    // Case 4: shuffle-regularize a zero tail, then re-enter the AST driver.
    if word.last().is_some_and(Rat::is_zero) {
        let substitute = if var.equal(&one) {
            Expr::leaf(Rat::zero(ctx.clone()))
        } else {
            Expr::hlog(var.clone(), Word::new(vec![Rat::zero(ctx.clone())]))
        };
        let terms = reg_tail_expr(
            &Word::new(word.to_vec()),
            &Rat::zero(ctx.clone()),
            &substitute,
            ctx,
        )?;
        let mut summands = Vec::with_capacity(terms.len());
        for term in terms {
            summands.push(Expr::times(vec![
                term.coef,
                Expr::hlog(var.clone(), term.word),
            ]));
        }
        let sum = if summands.len() == 1 {
            summands.pop().expect("one summand")
        } else {
            Expr::plus(summands)
        };
        return convert_to_hlog_reg_inf(&sum, ctx);
    }

    // Case 5: no regularization exists for a leading letter at the endpoint.
    if word.first().is_some_and(|letter| letter.equal(var)) {
        return Err(conversion_error(
            "Hlog[var, [var, ...]] is divergent ($Failed in Mma; no regularization available)",
        ));
    }

    // Case 6: a constant non-zero word is a log power.
    if word.iter().all(|letter| letter.equal(&word[0])) {
        let repeated_letter = var.try_div(&word[0])?.try_sub(&one)?;
        return log_power(ctx, repeated_letter, word.len());
    }

    // Case 7: general convergent word.
    case_general(var, word, ctx)
}

fn identity_regulator(ctx: &Arc<PolyCtx>) -> Regulator {
    vec![RegTerm {
        coef: Rat::one(ctx.clone()),
        key: RegKey::new(),
    }]
}

/// Convert an expression tree into a collected sum of regulator monomials.
pub fn convert_to_hlog_reg_inf(expr: &Expr, ctx: &Arc<PolyCtx>) -> Result<Regulator> {
    match expr {
        Expr::Leaf(value) => {
            require_rat_context(value, ctx)?;
            if value.is_zero() {
                Ok(Regulator::new())
            } else {
                Ok(vec![RegTerm {
                    coef: value.clone(),
                    key: RegKey::new(),
                }])
            }
        }
        Expr::Hlog { arg, word } => convert_to_hlog_reg_inf_hlog(arg, &word.letters, ctx),
        Expr::Plus(children) => {
            let mut output = Regulator::new();
            for child in children {
                output.extend(convert_to_hlog_reg_inf(child, ctx)?);
                output = canonicalize_regulator(&output)?;
            }
            Ok(output)
        }
        Expr::Times(children) => {
            let mut output = identity_regulator(ctx);
            for child in children {
                let factor = convert_to_hlog_reg_inf(child, ctx)?;
                output = shuffle_symbolic(&output, &factor)?;
            }
            canonicalize_regulator(&output)
        }
        Expr::Power(base, exponent) => {
            if *exponent < 1 {
                return Err(conversion_error(
                    "Power node has a non-positive exponent (the parser should have folded it)",
                ));
            }
            let base = convert_to_hlog_reg_inf(base, ctx)?;
            let mut output = identity_regulator(ctx);
            for _ in 0..*exponent {
                output = shuffle_symbolic(&output, &base)?;
            }
            canonicalize_regulator(&output)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Arc<PolyCtx> {
        PolyCtx::new(["x", "a"]).unwrap()
    }

    fn rat(ctx: &Arc<PolyCtx>, expression: &str) -> Rat {
        Rat::parse(ctx.clone(), expression).unwrap()
    }

    #[test]
    fn seven_case_order_handles_empty_zero_and_divergent_inputs() {
        let ctx = context();
        let zero = rat(&ctx, "0");
        let one = rat(&ctx, "1");
        let x = rat(&ctx, "x");

        let identity = convert_to_hlog_reg_inf_hlog(&zero, &[], &ctx).unwrap();
        assert_eq!(identity.len(), 1);
        assert!(identity[0].coef.is_one());
        assert!(identity[0].key.is_empty());

        assert!(
            convert_to_hlog_reg_inf_hlog(&zero, std::slice::from_ref(&one), &ctx)
                .unwrap()
                .is_empty()
        );
        assert!(
            convert_to_hlog_reg_inf_hlog(&one, &[zero.clone(), zero], &ctx)
                .unwrap()
                .is_empty()
        );

        let error = convert_to_hlog_reg_inf_hlog(&x, &[x.clone(), one], &ctx).unwrap_err();
        assert!(error.to_string().contains("divergent"));
    }

    #[test]
    fn repeated_nonzero_letters_use_the_log_power_formula() {
        let ctx = context();
        let x = rat(&ctx, "x");
        let a = rat(&ctx, "a");
        let result = convert_to_hlog_reg_inf_hlog(&x, &[a.clone(), a], &ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].coef, rat(&ctx, "1/2"));
        assert_eq!(
            result[0].key,
            vec![Word::new(vec![rat(&ctx, "x/a-1"), rat(&ctx, "x/a-1")])]
        );
    }

    #[test]
    fn general_case_matches_the_upstream_concat_order() {
        let ctx = context();
        let result =
            convert_to_hlog_reg_inf_hlog(&rat(&ctx, "x"), &[rat(&ctx, "a"), rat(&ctx, "1")], &ctx)
                .unwrap();
        let minus_one = rat(&ctx, "-1");
        let first_letter = rat(&ctx, "x-1");
        let second_letter = rat(&ctx, "x/a-1");
        assert_eq!(
            result,
            vec![
                RegTerm {
                    coef: rat(&ctx, "1"),
                    key: vec![Word::new(vec![minus_one.clone(), minus_one.clone()])],
                },
                RegTerm {
                    coef: rat(&ctx, "-1"),
                    key: vec![Word::new(vec![minus_one.clone(), second_letter.clone()])],
                },
                RegTerm {
                    coef: rat(&ctx, "-1"),
                    key: vec![Word::new(vec![first_letter.clone(), minus_one])],
                },
                RegTerm {
                    coef: rat(&ctx, "1"),
                    key: vec![Word::new(vec![first_letter, second_letter])],
                },
            ]
        );
    }

    #[test]
    fn trailing_zero_regularization_never_reemits_a_zero_tail() {
        let ctx = context();
        let word = Word::new(vec![rat(&ctx, "a"), rat(&ctx, "0"), rat(&ctx, "0")]);
        let substitute = Expr::hlog(rat(&ctx, "x"), Word::new(vec![rat(&ctx, "0")]));
        let result = reg_tail_expr(&word, &rat(&ctx, "0"), &substitute, &ctx).unwrap();

        assert!(!result.is_empty());
        assert!(result.iter().all(|term| {
            term.word
                .letters
                .last()
                .is_some_and(|letter| !letter.is_zero())
        }));
    }

    #[test]
    fn expression_driver_adds_and_multiplies_regulators() {
        let ctx = context();
        let hlog = Expr::hlog(rat(&ctx, "x"), Word::new(vec![rat(&ctx, "a")]));
        let expression = Expr::times(vec![
            Expr::plus(vec![Expr::leaf(rat(&ctx, "2")), Expr::leaf(rat(&ctx, "3"))]),
            hlog,
        ]);
        let result = convert_to_hlog_reg_inf(&expression, &ctx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].coef, rat(&ctx, "-5"));
        assert_eq!(result[0].key.len(), 1);
        assert_eq!(result[0].key[0], Word::new(vec![rat(&ctx, "x/a-1")]));
    }
}
