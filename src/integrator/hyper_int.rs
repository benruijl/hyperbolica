use std::collections::HashSet;
use std::sync::Arc;

use crate::algebra::algebraic_letters::join_algebraic_letter_session;
use crate::core::{Poly, PolyCtx, Rat, SymCoef};
use crate::error::Error;
use crate::reduce::{MzvReductionTable, substitute_var_rat};

use super::integration_step::{close_positive_letters, integration_step_core_sym_with_options};
use super::{
    IntegrationResult, IntegrationStepOptions, RegTermSym, RegulatorSym, ShuffleEntry,
    ShuffleEntrySym, ShuffleList, ShuffleListSym, canonicalize_regkey, canonicalize_regulator_sym,
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
        let coefficient = entry.materialized_coefficient()?;
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
            factored_coefficient: None,
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
    hyper_int_with_options_and_spectators(ctx, input, variables, table, options, &[])
}

/// Integrate a schedule while retaining never-integrated user variables in
/// endpoint-divergence zero tests.
///
/// A boundary bin can vanish only after identities depending on a surviving
/// kinematic parameter are combined. Projecting over the union of later
/// integration variables and these spectators matches that mathematical
/// contract without treating generated MZV/algebraic constants as variables.
/// Only later scheduled variables guard algebraic-letter introduction;
/// never-integrated spectators remain valid parameters of a formal root.
pub fn hyper_int_with_options_and_spectators(
    ctx: &Arc<PolyCtx>,
    input: &ShuffleList,
    variables: &[usize],
    table: &MzvReductionTable,
    options: &HyperIntOptions,
    spectator_variables: &[usize],
) -> IntegrationResult<RegulatorSym> {
    let mut scheduled = HashSet::with_capacity(variables.len());
    for &variable in variables {
        if variable >= ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()).into());
        }
        if !scheduled.insert(variable) {
            return Err(Error::InvalidInput(format!(
                "integration variable `{}` is listed more than once",
                ctx.vars()[variable]
            ))
            .into());
        }
    }
    let mut spectators = HashSet::with_capacity(spectator_variables.len());
    for &spectator in spectator_variables {
        if spectator >= ctx.len() {
            return Err(Error::UnknownVariable(spectator.to_string()).into());
        }
        if !spectators.insert(spectator) {
            return Err(Error::InvalidInput(format!(
                "spectator variable `{}` is listed more than once",
                ctx.vars()[spectator]
            ))
            .into());
        }
        if scheduled.contains(&spectator) {
            return Err(Error::InvalidInput(format!(
                "spectator variable `{}` is also in the integration schedule",
                ctx.vars()[spectator]
            ))
            .into());
        }
    }
    if variables.is_empty() {
        return input_as_regulator(input);
    }
    let _algebraic_session = options
        .step
        .introduce_algebraic_letters
        .then(join_algebraic_letter_session)
        .transpose()?;

    let mut current = input
        .iter()
        .map(|entry| ShuffleEntrySym {
            coef: SymCoef::from_rat(&entry.coef),
            shuffle: entry.shuffle.clone(),
            factored_coefficient: entry.factored_coefficient().cloned(),
        })
        .collect::<Vec<_>>();
    for (step, &variable) in variables.iter().enumerate() {
        let final_step = step + 1 == variables.len();
        let remaining_integration_variables = &variables[step + 1..];
        let mut fibration_variables = remaining_integration_variables.to_vec();
        for &spectator in spectator_variables {
            if !fibration_variables.contains(&spectator) {
                fibration_variables.push(spectator);
            }
        }
        let base = integration_step_core_sym_with_options(
            ctx,
            &current,
            variable,
            table,
            &options.step,
            remaining_integration_variables,
            &fibration_variables,
        )?;
        let result = if final_step && options.close_final_positive_letters {
            close_positive_letters(ctx, &base, variable, table)?
        } else {
            base
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

/// Project-name alias for [`hyper_int_with_options_and_spectators`].
pub fn hyperflint_with_options_and_spectators(
    ctx: &Arc<PolyCtx>,
    input: &ShuffleList,
    variables: &[usize],
    table: &MzvReductionTable,
    options: &HyperIntOptions,
    spectator_variables: &[usize],
) -> IntegrationResult<RegulatorSym> {
    hyper_int_with_options_and_spectators(
        ctx,
        input,
        variables,
        table,
        options,
        spectator_variables,
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Bound {
    Finite(Rat),
    PositiveInfinity,
    NegativeInfinity,
}

fn parse_bound_with(
    expression: &str,
    parse_finite: impl FnOnce(&str) -> Result<Rat, Error>,
) -> Result<Bound, Error> {
    match expression {
        "Infinity" | "+Infinity" | "oo" | "+oo" => Ok(Bound::PositiveInfinity),
        "-Infinity" | "-oo" => Ok(Bound::NegativeInfinity),
        _ => Ok(Bound::Finite(parse_finite(expression)?)),
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
    let coefficient = entry.materialized_coefficient()?;
    Ok(ShuffleEntry::new(
        substitute_var_rat(&coefficient, variable, replacement)?.try_mul(jacobian)?,
        shuffle,
    ))
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
    rescale_interval_with_bound_parser(ctx, input, variable, from, to, |expression| {
        Rat::parse(ctx.clone(), expression)
    })
}

/// Compatibility-boundary variant of [`rescale_interval`] whose finite bounds
/// are decoded by the caller.
///
/// The typed core parser deliberately accepts Symbolica spellings. The JSON
/// bridge supplies its legacy-name adapter here so transport aliases resolve
/// to the same registered atoms already present in `ctx`.
pub(crate) fn rescale_interval_with_bound_parser(
    ctx: &Arc<PolyCtx>,
    input: &ShuffleList,
    variable: usize,
    from: &str,
    to: &str,
    parse_finite: impl Fn(&str) -> Result<Rat, Error>,
) -> IntegrationResult<ShuffleList> {
    let variable_rat = Rat::from_poly(Poly::generator(ctx.clone(), variable)?);
    let one = Rat::one(ctx.clone());
    let minus_one = Rat::from_int(ctx.clone(), -1);
    let from = parse_bound_with(from, &parse_finite)?;
    let to = parse_bound_with(to, &parse_finite)?;

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
    fn spectator_projection_accepts_upstream_cross_letter_regression() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let input = vec![ShuffleEntry::new(
            Rat::parse(ctx.clone(), "1/((x+1)*(x+y))").unwrap(),
            Vec::new(),
        )];
        let options = HyperIntOptions {
            step: IntegrationStepOptions {
                check_divergences: true,
                parallel: false,
                ..IntegrationStepOptions::default()
            },
            ..HyperIntOptions::default()
        };

        // The x-boundary bins cancel only as functions of the surviving
        // parameter y. Testing them term-by-term falsely reports divergence.
        let result = hyper_int_with_options_and_spectators(
            &ctx,
            &input,
            &[0],
            &MzvReductionTable::default(),
            &options,
            &[1],
        )
        .unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn public_index_driver_rejects_duplicate_schedule_and_spectators() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let input = Vec::<ShuffleEntry>::new();
        let options = HyperIntOptions::default();

        let duplicate_schedule = hyper_int_with_options_and_spectators(
            &ctx,
            &input,
            &[0, 0],
            &MzvReductionTable::default(),
            &options,
            &[],
        )
        .unwrap_err();
        assert!(
            duplicate_schedule
                .to_string()
                .contains("integration variable `x` is listed more than once")
        );

        let duplicate_spectator = hyper_int_with_options_and_spectators(
            &ctx,
            &input,
            &[0],
            &MzvReductionTable::default(),
            &options,
            &[1, 1],
        )
        .unwrap_err();
        assert!(
            duplicate_spectator
                .to_string()
                .contains("spectator variable `y` is listed more than once")
        );
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
