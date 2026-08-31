//! Contour splitting for period and positive-letter branches.

use std::sync::Arc;

use crate::algebra::convert::convert_ab_to_zero_infinity;
use crate::algebra::shuffle::shuffle_product;
use crate::core::{PolyCtx, Rat, SymCoef};
use crate::error::{Error, Result};
use crate::integrator::{
    RegKey, RegTerm, RegTermSym, Regulator, RegulatorSym, canonicalize_regulator,
    canonicalize_regulator_sym, reg_head,
};
use crate::symbols::{Word, Wordlist, WordlistTerm};

use super::mzv_reduce::MzvReductionTable;
use super::periods::{zero_inf_period, zero_one_period};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnAxisEntry {
    pub letter: Rat,
    pub im_side: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnAxisSymEntry {
    pub letter: Rat,
    pub im_part: SymCoef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WordlistSymTerm {
    pub coef: SymCoef,
    pub word: Word,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WordlistSym {
    pub terms: Vec<WordlistSymTerm>,
}

pub fn to_wordlist_sym(wordlist: &Wordlist) -> WordlistSym {
    WordlistSym {
        terms: wordlist
            .terms
            .iter()
            .map(|term| WordlistSymTerm {
                coef: SymCoef::from_rat(&term.coef),
                word: term.word.clone(),
            })
            .collect(),
    }
}

fn period_word(word: &Word) -> bool {
    word.letters
        .iter()
        .all(|letter| matches!(letter.to_string().as_str(), "-2" | "-1" | "0"))
}

/// Base contour split used when no positive singularity lies on the path.
pub fn break_up_contour(
    ctx: &Arc<PolyCtx>,
    wordlist: &Wordlist,
    on_axis: &[OnAxisEntry],
    table: &MzvReductionTable,
) -> Result<Regulator> {
    if !on_axis.is_empty() {
        return Err(Error::InvalidInput(
            "break_up_contour: non-empty on-axis list requires break_up_contour_sym".into(),
        ));
    }

    let mut constant = Rat::zero(ctx.clone());
    let mut output = Regulator::new();
    for term in &wordlist.terms {
        if period_word(&term.word) {
            let period = zero_inf_period(ctx, &term.word, table)?;
            constant = constant.try_add(&term.coef.try_mul(&period)?)?;
        } else {
            output.push(RegTerm {
                coef: term.coef.clone(),
                key: if term.word.is_empty() {
                    RegKey::new()
                } else {
                    vec![term.word.clone()]
                },
            });
        }
    }
    if !constant.is_zero() {
        output.push(RegTerm {
            coef: constant,
            key: RegKey::new(),
        });
    }
    canonicalize_regulator(&output)
}

fn collect_wordlist_sym(wordlist: WordlistSym) -> Result<WordlistSym> {
    let mut output = Vec::<WordlistSymTerm>::new();
    let mut keys = Vec::<String>::new();
    for term in wordlist.terms {
        let key = term.word.content_key();
        if let Some(index) = keys.iter().position(|candidate| candidate == &key) {
            output[index].coef = output[index].coef.try_add(&term.coef)?;
        } else {
            keys.push(key);
            output.push(term);
        }
    }
    output.retain(|term| !term.coef.is_zero());
    Ok(WordlistSym { terms: output })
}

fn factorial_rat(ctx: &Arc<PolyCtx>, value: usize) -> Result<Rat> {
    let mut factorial = 1_i64;
    for next in 2..=value {
        factorial = factorial
            .checked_mul(next as i64)
            .ok_or_else(|| Error::InvalidInput("break_up_contour: factorial overflow".into()))?;
    }
    Ok(Rat::from_int(ctx.clone(), factorial))
}

fn reg_tail_sym(
    ctx: &Arc<PolyCtx>,
    wordlist: &WordlistSym,
    letter: &Rat,
    substitute: &SymCoef,
) -> Result<WordlistSym> {
    let mut result = WordlistSym::default();
    for term in &wordlist.terms {
        if term.word.is_empty() {
            result.terms.push(term.clone());
            continue;
        }
        let run = term
            .word
            .letters
            .iter()
            .rev()
            .take_while(|candidate| candidate.equal(letter))
            .count();
        if run == term.word.len() {
            let mut prefactor = SymCoef::one(ctx.clone());
            for _ in 0..run {
                prefactor = prefactor.try_mul(substitute)?;
            }
            if run != 0 {
                prefactor = prefactor.try_div_rat(&factorial_rat(ctx, run)?)?;
            }
            result.terms.push(WordlistSymTerm {
                coef: term.coef.try_mul(&prefactor)?,
                word: Word::default(),
            });
            continue;
        }

        let anchor_index = term.word.len() - run - 1;
        let anchor = term.word[anchor_index].clone();
        let stripped = Word::new(term.word.letters[..anchor_index].to_vec());
        for shuffle_length in 0..=run {
            let sign = if shuffle_length % 2 == 0 { 1 } else { -1 };
            let left = Wordlist::new(vec![WordlistTerm::new(
                Rat::from_int(ctx.clone(), sign),
                Word::new(vec![letter.clone(); shuffle_length]),
            )]);
            let right = Wordlist::new(vec![WordlistTerm::new(
                Rat::one(ctx.clone()),
                stripped.clone(),
            )]);
            let shuffled = shuffle_product(&left, &right);

            let mut prefactor = SymCoef::one(ctx.clone());
            if shuffle_length != run {
                for _ in 0..(run - shuffle_length) {
                    prefactor = prefactor.try_mul(substitute)?;
                }
                prefactor = prefactor.try_div_rat(&factorial_rat(ctx, run - shuffle_length)?)?;
            }
            let combined = term.coef.try_mul(&prefactor)?;
            for shuffled_term in shuffled.terms {
                let mut word = shuffled_term.word;
                word.letters.push(anchor.clone());
                result.terms.push(WordlistSymTerm {
                    coef: combined.try_mul_rat(&shuffled_term.coef)?,
                    word,
                });
            }
        }
    }
    collect_wordlist_sym(result)
}

fn numeric_word(word: &Word) -> bool {
    word.letters.iter().all(|letter| {
        letter.numerator().is_rational_constant() && letter.denominator().is_rational_constant()
    })
}

/// Full SymCoef-valued recursive contour deformation.
pub fn break_up_contour_sym(
    ctx: &Arc<PolyCtx>,
    wordlist: &WordlistSym,
    on_axis: &[OnAxisSymEntry],
    table: &MzvReductionTable,
) -> Result<RegulatorSym> {
    if on_axis.is_empty() {
        let mut constant = SymCoef::zero(ctx.clone());
        let mut output = RegulatorSym::new();
        for term in &wordlist.terms {
            if period_word(&term.word) {
                let period = zero_inf_period(ctx, &term.word, table)?;
                constant = constant.try_add(&term.coef.try_mul_rat(&period)?)?;
            } else {
                output.push(RegTermSym {
                    coef: term.coef.clone(),
                    key: if term.word.is_empty() {
                        RegKey::new()
                    } else {
                        vec![term.word.clone()]
                    },
                });
            }
        }
        if !constant.is_zero() {
            output.push(RegTermSym {
                coef: constant,
                key: RegKey::new(),
            });
        }
        return canonicalize_regulator_sym(&output);
    }

    let smallest = &on_axis[0].letter;
    let smallest_integer = smallest.to_string().parse::<i64>().map_err(|_| {
        Error::InvalidInput(format!(
            "break_up_contour_sym: symbolic or non-integral smallest letter `{smallest}`"
        ))
    })?;
    if smallest_integer <= 0 {
        return Err(Error::InvalidInput(format!(
            "break_up_contour_sym: smallest letter must be positive, got {smallest_integer}"
        )));
    }

    let mut new_axis = Vec::with_capacity(on_axis.len() - 1);
    for entry in &on_axis[1..] {
        new_axis.push(OnAxisSymEntry {
            letter: entry.letter.try_sub(smallest)?,
            im_part: entry.im_part.clone(),
        });
    }

    let pi_i = SymCoef::pi_factor(ctx.clone()).try_mul(&SymCoef::im_factor(ctx.clone()))?;
    let mut substitute = pi_i.try_mul(&on_axis[0].im_part)?;
    if smallest_integer != 1 {
        substitute = substitute.try_sub(&SymCoef::log_factor(ctx.clone(), smallest_integer)?)?;
    }

    let mut output = RegulatorSym::new();
    for term in &wordlist.terms {
        for split in 0..=term.word.len() {
            let tail = Word::new(term.word.letters[split..].to_vec());
            let mut temporary = RegulatorSym::new();
            if smallest_integer == 1 && numeric_word(&tail) {
                let period = zero_one_period(ctx, &tail, table)?;
                if !period.is_zero() {
                    temporary.push(RegTermSym {
                        coef: SymCoef::from_rat(&period),
                        key: RegKey::new(),
                    });
                }
            } else {
                let seed = Wordlist::new(vec![WordlistTerm::new(Rat::one(ctx.clone()), tail)]);
                let regularized = reg_head(&seed, smallest, &Rat::zero(ctx.clone()))?;
                let converted =
                    convert_ab_to_zero_infinity(&regularized, &Rat::zero(ctx.clone()), smallest)?;
                temporary.extend(converted.terms.into_iter().map(|entry| RegTermSym {
                    coef: SymCoef::from_rat(&entry.coef),
                    key: if entry.word.is_empty() {
                        RegKey::new()
                    } else {
                        vec![entry.word]
                    },
                }));
            }

            let head = Word::new(
                term.word.letters[..split]
                    .iter()
                    .map(|letter| letter.try_sub(smallest))
                    .collect::<Result<Vec<_>>>()?,
            );
            let head_seed = WordlistSym {
                terms: vec![WordlistSymTerm {
                    coef: term.coef.clone(),
                    word: head,
                }],
            };
            let regularized_head =
                reg_tail_sym(ctx, &head_seed, &Rat::zero(ctx.clone()), &substitute)?;
            let broken = break_up_contour_sym(ctx, &regularized_head, &new_axis, table)?;
            for left in &broken {
                for right in &temporary {
                    let mut key = left.key.clone();
                    key.extend(right.key.iter().cloned());
                    output.push(RegTermSym {
                        coef: left.coef.try_mul(&right.coef)?,
                        key,
                    });
                }
            }
        }
    }
    canonicalize_regulator_sym(&output)
}

#[cfg(test)]
mod tests {
    use symbolica::prelude::Symbol;

    use super::*;
    use crate::core::Poly;
    use crate::reduce::{MzvReductionRule, build_mzv_atom_list};
    use crate::symbols::{SYMBOL_NAMESPACE, mzv_atom};

    fn setup() -> (Arc<PolyCtx>, MzvReductionTable) {
        let table = MzvReductionTable {
            reductions: vec![MzvReductionRule {
                lhs: "mzv_4".into(),
                rhs: "2/5*mzv_2^2".into(),
            }],
            basis: vec!["Log2".into(), "mzv_2".into()],
        };
        let x = Symbol::parse("x", SYMBOL_NAMESPACE).unwrap();
        let ctx = PolyCtx::from_indeterminates(build_mzv_atom_list(&table, [x.to_atom()]).unwrap())
            .unwrap();
        (ctx, table)
    }

    #[test]
    fn base_case_folds_period_words_and_preserves_parametric_words() {
        let (ctx, table) = setup();
        let input = Wordlist::new(vec![
            WordlistTerm::new(
                Rat::from_int(ctx.clone(), 2),
                Word::new(vec![Rat::zero(ctx.clone()), Rat::one(ctx.clone())]),
            ),
            WordlistTerm::new(
                Rat::one(ctx.clone()),
                Word::new(vec![Rat::parse(ctx.clone(), "x").unwrap()]),
            ),
        ]);
        let result = break_up_contour(&ctx, &input, &[], &table).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(
            result.iter().find(|term| term.key.is_empty()).unwrap().coef,
            Rat::from_poly(
                Poly::generator(
                    ctx.clone(),
                    ctx.index_of_indeterminate(mzv_atom(&[2]).as_view())
                        .unwrap(),
                )
                .unwrap(),
            )
            .try_mul(&Rat::from_int(ctx, -2))
            .unwrap()
        );
    }

    #[test]
    fn symbolic_base_case_promotes_coefficients_without_loss() {
        let (ctx, table) = setup();
        let input = Wordlist::new(vec![WordlistTerm::new(
            Rat::one(ctx.clone()),
            Word::new(vec![Rat::parse(ctx.clone(), "x").unwrap()]),
        )]);
        let result = break_up_contour_sym(&ctx, &to_wordlist_sym(&input), &[], &table).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].coef, SymCoef::one(ctx));
    }
}
