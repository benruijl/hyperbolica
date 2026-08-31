//! Differentiation of typed hyperlogarithm and multiple-polylogarithm symbols.

use crate::core::Rat;
use crate::error::{Error, Result};
use crate::symbols::{Hlog, Mpl, Word};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HlogTerm {
    pub coef: Rat,
    pub hlog: Hlog,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MplTerm {
    pub coef: Rat,
    pub mpl: Mpl,
}

fn delete_letter(word: &Word, index: usize) -> Word {
    Word::new(
        word.letters
            .iter()
            .enumerate()
            .filter(|(position, _)| *position != index)
            .map(|(_, letter)| letter.clone())
            .collect(),
    )
}

fn push_hlog(output: &mut Vec<HlogTerm>, coefficient: Rat, z: &Rat, word: Word) {
    if !coefficient.is_zero() {
        output.push(HlogTerm {
            coef: coefficient,
            hlog: Hlog::new(z.clone(), word),
        });
    }
}

pub fn diff_hlog(z: &Rat, word: &Word, variable: usize) -> Result<Vec<HlogTerm>> {
    if word.is_empty() {
        return Ok(Vec::new());
    }
    let derivatives = word
        .letters
        .iter()
        .map(|letter| letter.derivative(variable))
        .collect::<Result<Vec<_>>>()?;
    let dz = z.derivative(variable)?;
    let mut output = Vec::new();

    let first_denominator = z.try_sub(&word[0])?;
    if !first_denominator.is_zero() && !dz.is_zero() {
        push_hlog(
            &mut output,
            dz.try_div(&first_denominator)?,
            z,
            Word::new(word.letters[1..].to_vec()),
        );
    }

    for index in 0..word.len() - 1 {
        let difference = word[index].try_sub(&word[index + 1])?;
        if difference.is_zero() {
            continue;
        }
        let derivative = derivatives[index].try_sub(&derivatives[index + 1])?;
        if derivative.is_zero() {
            continue;
        }
        let coefficient = derivative.try_div(&difference)?;
        push_hlog(
            &mut output,
            coefficient.clone(),
            z,
            delete_letter(word, index + 1),
        );
        push_hlog(
            &mut output,
            coefficient.negated(),
            z,
            delete_letter(word, index),
        );
    }

    if !word[word.len() - 1].is_zero() && !derivatives[word.len() - 1].is_zero() {
        push_hlog(
            &mut output,
            derivatives[word.len() - 1]
                .try_div(&word[word.len() - 1])?
                .negated(),
            z,
            Word::new(word.letters[..word.len() - 1].to_vec()),
        );
    }

    if !first_denominator.is_zero() && !derivatives[0].is_zero() {
        push_hlog(
            &mut output,
            derivatives[0].try_div(&first_denominator)?.negated(),
            z,
            Word::new(word.letters[1..].to_vec()),
        );
    }
    Ok(output)
}

pub fn diff_mpl(indices: &[i64], args: &[Rat], variable: usize) -> Result<Vec<MplTerm>> {
    if indices.len() != args.len() {
        return Err(Error::InvalidInput(
            "Mpl index and argument lengths differ".into(),
        ));
    }
    if indices.is_empty() {
        return Ok(Vec::new());
    }
    let ctx = args[0].ctx().clone();
    let one = Rat::one(ctx);
    let mut output = Vec::new();

    for index in 0..indices.len() {
        let derivative = args[index].derivative(variable)?;
        if derivative.is_zero() {
            continue;
        }
        if indices[index] > 1 {
            let mut lowered = indices.to_vec();
            lowered[index] -= 1;
            output.push(MplTerm {
                coef: derivative.try_div(&args[index])?,
                mpl: Mpl::new(lowered, args.to_vec()),
            });
            continue;
        }
        if indices[index] < 1 {
            return Err(Error::InvalidInput(
                "diff_mpl does not support non-positive indices".into(),
            ));
        }

        if indices.len() == 1 {
            output.push(MplTerm {
                coef: derivative.try_div(&one.try_sub(&args[0])?)?,
                mpl: Mpl::new(Vec::new(), Vec::new()),
            });
            continue;
        }

        if index == indices.len() - 1 {
            let mut new_args = args[..args.len() - 1].to_vec();
            *new_args.last_mut().unwrap() = args[args.len() - 2].try_mul(&args[args.len() - 1])?;
            output.push(MplTerm {
                coef: derivative.try_div(&one.try_sub(&args[index])?)?,
                mpl: Mpl::new(indices[..indices.len() - 1].to_vec(), new_args),
            });
            continue;
        }

        let shortened_indices = indices
            .iter()
            .enumerate()
            .filter(|(position, _)| *position != index)
            .map(|(_, value)| *value)
            .collect::<Vec<_>>();
        let shortened_args = args
            .iter()
            .enumerate()
            .filter(|(position, _)| *position != index)
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>();
        let z_minus_one = args[index].try_sub(&one)?;
        let first_coefficient = derivative.try_div(&args[index].try_mul(&z_minus_one)?)?;
        let second_coefficient = derivative.try_div(&z_minus_one)?.negated();

        if index == 0 {
            let mut merged_right = shortened_args.clone();
            merged_right[0] = args[0].try_mul(&args[1])?;
            output.push(MplTerm {
                coef: first_coefficient,
                mpl: Mpl::new(shortened_indices.clone(), merged_right),
            });
            output.push(MplTerm {
                coef: second_coefficient,
                mpl: Mpl::new(shortened_indices, shortened_args),
            });
        } else {
            let mut merged_right = shortened_args.clone();
            merged_right[index] = args[index].try_mul(&args[index + 1])?;
            let mut merged_left = shortened_args;
            merged_left[index - 1] = args[index - 1].try_mul(&args[index])?;
            output.push(MplTerm {
                coef: first_coefficient,
                mpl: Mpl::new(shortened_indices.clone(), merged_right),
            });
            output.push(MplTerm {
                coef: second_coefficient,
                mpl: Mpl::new(shortened_indices, merged_left),
            });
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use crate::core::PolyCtx;

    use super::*;

    #[test]
    fn differentiates_weight_one_polylogarithm() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let x = Rat::parse(ctx.clone(), "x").unwrap();
        let result = diff_mpl(&[1], std::slice::from_ref(&x), 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].coef, Rat::parse(ctx, "1/(1-x)").unwrap());
        assert!(result[0].mpl.indices.is_empty());
    }

    #[test]
    fn constant_hlog_has_zero_derivative() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        assert!(
            diff_hlog(&Rat::parse(ctx, "x").unwrap(), &Word::default(), 0)
                .unwrap()
                .is_empty()
        );
    }
}
