use std::sync::Arc;

use crate::core::{Poly, PolyCtx, Rat, SymCoef};
use crate::error::Error;
use crate::reduce::{MzvReductionTable, substitute_var_rat};

use super::integration_step::integration_step_core_sym_with_options;
use super::{
    IntegrationResult, IntegrationStepOptions, RegTermSym, RegulatorSym, ShuffleEntry,
    ShuffleEntrySym, ShuffleList, ShuffleListSym, canonicalize_regkey, canonicalize_regulator_sym,
    integration_step_sym_with_options,
};

#[derive(Clone, Debug)]
pub struct HyperIntOptions {
    pub step: IntegrationStepOptions,
    /// Apply the positive-real-axis contour closure after the last step.
    pub close_final_positive_letters: bool,
}

impl Default for HyperIntOptions {
    fn default() -> Self {
        Self {
            step: IntegrationStepOptions::default(),
            close_final_positive_letters: true,
        }
    }
}

fn input_as_regulator(input: &ShuffleList) -> IntegrationResult<RegulatorSym> {
    let mut output = RegulatorSym::with_capacity(input.len());
    for entry in input {
        let coefficient = match &entry.factored_den {
            Some(factored) => factored.materialize()?,
            None => entry.coef.clone(),
        };
        output.push(RegTermSym {
            coef: SymCoef::from_rat(&coefficient),
            key: canonicalize_regkey(&entry.shuffle),
        });
    }
    Ok(canonicalize_regulator_sym(&output)?)
}

fn regulator_as_shuffle_list(regulator: RegulatorSym) -> ShuffleListSym {
    regulator
        .into_iter()
        .map(|term| ShuffleEntrySym {
            coef: term.coef,
            shuffle: term.key,
            factored_den: None,
        })
        .collect()
}

/// Integrate an input over the scheduled variables, each from zero to
/// infinity.
pub fn hyper_int(
    ctx: &Arc<PolyCtx>,
    input: &ShuffleList,
    variables: &[usize],
    table: &MzvReductionTable,
) -> IntegrationResult<RegulatorSym> {
    hyper_int_with_options(ctx, input, variables, table, &HyperIntOptions::default())
}

pub fn hyper_int_with_options(
    ctx: &Arc<PolyCtx>,
    input: &ShuffleList,
    variables: &[usize],
    table: &MzvReductionTable,
    options: &HyperIntOptions,
) -> IntegrationResult<RegulatorSym> {
    for &variable in variables {
        if variable >= ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()).into());
        }
    }
    if variables.is_empty() {
        return input_as_regulator(input);
    }

    let mut current = input
        .iter()
        .map(|entry| ShuffleEntrySym {
            coef: SymCoef::from_rat(&entry.coef),
            shuffle: entry.shuffle.clone(),
            factored_den: entry.factored_den.clone(),
        })
        .collect::<Vec<_>>();
    for (step, &variable) in variables.iter().enumerate() {
        let final_step = step + 1 == variables.len();
        let result = if final_step && options.close_final_positive_letters {
            integration_step_sym_with_options(ctx, &current, variable, table, &options.step)?
        } else {
            integration_step_core_sym_with_options(
                ctx,
                &current,
                variable,
                table,
                &options.step,
                &variables[step + 1..],
            )?
        };
        if result.is_empty() {
            return Ok(result);
        }
        if final_step {
            return Ok(canonicalize_regulator_sym(&result)?);
        }
        current = regulator_as_shuffle_list(result);
    }
    unreachable!("the non-empty integration schedule returns from its final step")
}

/// Project-name alias for [`hyper_int`].
pub fn hyperflint(
    ctx: &Arc<PolyCtx>,
    input: &ShuffleList,
    variables: &[usize],
    table: &MzvReductionTable,
) -> IntegrationResult<RegulatorSym> {
    hyper_int(ctx, input, variables, table)
}

/// Upstream-compatible name emphasizing the SymCoef-valued result.
pub fn hyperflint_sym(
    ctx: &Arc<PolyCtx>,
    input: &ShuffleList,
    variables: &[usize],
    table: &MzvReductionTable,
) -> IntegrationResult<RegulatorSym> {
    hyper_int(ctx, input, variables, table)
}

pub fn hyperflint_with_options(
    ctx: &Arc<PolyCtx>,
    input: &ShuffleList,
    variables: &[usize],
    table: &MzvReductionTable,
    options: &HyperIntOptions,
) -> IntegrationResult<RegulatorSym> {
    hyper_int_with_options(ctx, input, variables, table, options)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Bound {
    Finite(Rat),
    PositiveInfinity,
    NegativeInfinity,
}

fn parse_bound(ctx: &Arc<PolyCtx>, expression: &str) -> Result<Bound, Error> {
    match expression {
        "Infinity" | "+Infinity" | "oo" | "+oo" => Ok(Bound::PositiveInfinity),
        "-Infinity" | "-oo" => Ok(Bound::NegativeInfinity),
        _ => Ok(Bound::Finite(Rat::parse(ctx.clone(), expression)?)),
    }
}

fn rescale_entry(
    entry: &ShuffleEntry,
    variable: usize,
    replacement: &Rat,
    jacobian: &Rat,
) -> IntegrationResult<ShuffleEntry> {
    let mut shuffle = Vec::with_capacity(entry.shuffle.len());
    for word in &entry.shuffle {
        shuffle.push(crate::symbols::Word::new(
            word.letters
                .iter()
                .map(|letter| substitute_var_rat(letter, variable, replacement))
                .collect::<Result<Vec<_>, _>>()?,
        ));
    }
    let coefficient = match &entry.factored_den {
        Some(factored) => factored.materialize()?,
        None => entry.coef.clone(),
    };
    Ok(ShuffleEntry {
        coef: substitute_var_rat(&coefficient, variable, replacement)?.try_mul(jacobian)?,
        shuffle,
        factored_den: None,
    })
}

fn rescale_all(
    input: &ShuffleList,
    variable: usize,
    replacement: &Rat,
    jacobian: &Rat,
) -> IntegrationResult<ShuffleList> {
    input
        .iter()
        .map(|entry| rescale_entry(entry, variable, replacement, jacobian))
        .collect()
}

/// Map an arbitrary supported interval onto `[0, infinity)`.
pub fn rescale_interval(
    ctx: &Arc<PolyCtx>,
    input: &ShuffleList,
    variable: usize,
    from: &str,
    to: &str,
) -> IntegrationResult<ShuffleList> {
    let variable_rat = Rat::from_poly(Poly::generator(ctx.clone(), variable)?);
    let one = Rat::one(ctx.clone());
    let minus_one = Rat::from_int(ctx.clone(), -1);
    let from = parse_bound(ctx, from)?;
    let to = parse_bound(ctx, to)?;

    match (from, to) {
        (Bound::Finite(left), Bound::Finite(right)) if left.equal(&right) => Ok(Vec::new()),
        (Bound::Finite(left), Bound::PositiveInfinity) if left.is_zero() => Ok(input.clone()),
        (Bound::NegativeInfinity, Bound::PositiveInfinity) => {
            let mut output = input.clone();
            output.extend(rescale_all(input, variable, &variable_rat.negated(), &one)?);
            Ok(output)
        }
        (Bound::Finite(left), Bound::PositiveInfinity) => {
            rescale_all(input, variable, &left.try_add(&variable_rat)?, &one)
        }
        (Bound::NegativeInfinity, Bound::Finite(right)) => {
            rescale_all(input, variable, &right.try_sub(&variable_rat)?, &minus_one)
        }
        (Bound::Finite(left), Bound::NegativeInfinity) => {
            rescale_all(input, variable, &left.try_sub(&variable_rat)?, &minus_one)
        }
        (Bound::PositiveInfinity, Bound::Finite(right)) => {
            rescale_all(input, variable, &right.try_add(&variable_rat)?, &minus_one)
        }
        (Bound::Finite(left), Bound::Finite(right)) => {
            let one_plus_variable = one.try_add(&variable_rat)?;
            let replacement = left
                .try_add(&right.try_mul(&variable_rat)?)?
                .try_div(&one_plus_variable)?;
            let jacobian = right.try_sub(&left)?.try_div(&one_plus_variable.pow(2)?)?;
            rescale_all(input, variable, &replacement, &jacobian)
        }
        (from, to) => Err(Error::InvalidInput(format!(
            "unsupported interval bound combination: {from:?} -> {to:?}"
        ))
        .into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_variable_driver_integrates_in_schedule_order() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let input = vec![ShuffleEntry::new(
            Rat::parse(ctx.clone(), "1/((1+x)^2*(1+y)^2)").unwrap(),
            Vec::new(),
        )];
        let result = hyperflint(&ctx, &input, &[0, 1], &MzvReductionTable::default()).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].key.is_empty());
        assert_eq!(result[0].coef.as_rat().unwrap(), Rat::one(ctx));
    }

    #[test]
    fn empty_schedule_canonicalizes_input_without_integrating() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let input = vec![
            ShuffleEntry::new(Rat::from_int(ctx.clone(), 2), Vec::new()),
            ShuffleEntry::new(Rat::from_int(ctx.clone(), -1), Vec::new()),
        ];
        let result = hyper_int(&ctx, &input, &[], &MzvReductionTable::default()).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].coef.as_rat().unwrap(), Rat::one(ctx));
    }

    #[test]
    fn finite_interval_rescaling_includes_the_jacobian() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let input = vec![ShuffleEntry::new(
            Rat::parse(ctx.clone(), "x").unwrap(),
            Vec::new(),
        )];
        let output = rescale_interval(&ctx, &input, 0, "2", "5").unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(
            output[0].coef,
            Rat::parse(ctx, "3*(2+5*x)/(1+x)^3").unwrap()
        );
    }
}
