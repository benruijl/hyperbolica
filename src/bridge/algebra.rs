//! Polynomial, rational-function, and algebraic-letter bridge operations.

use std::sync::Arc;

use serde_json::{Value, json};

use super::wire::{
    array_field, context_for, context_for_variable, explicit_variables, optional_bool,
    parse_wire_poly, parse_wire_rat, parse_wire_rational_scalar, result_response, string_field,
    variable_index, wire_context_variables, wire_poly, wire_rat,
};
use crate::algebra::algebraic_letters::{
    DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE, algebraic_letters_allocate, algebraic_letters_clear,
    algebraic_letters_show, back_substitute, begin_algebraic_letter_session,
    build_algebraic_letter_atom_list, combine_wm_wp_ratios, join_algebraic_letter_session,
    simplify_with_vieta,
};
use crate::algebra::linear_factors::{LinearFactorOptions, linear_factors_with_options};
use crate::algebra::partial_fractions::{PartialFractionOptions, partial_fractions_with_options};
use crate::core::{PolyCtx, Rat};
use crate::error::{Error, Result};

pub(super) fn evaluate(request: &Value, op: &str) -> Option<Result<Value>> {
    matches!(
        op,
        "algebraic_letters_clear"
            | "algebraic_letters_show"
            | "algebraic_letters_allocate"
            | "simplify_with_vieta"
            | "back_substitute"
            | "combine_wm_wp_ratios"
            | "factor"
            | "add"
            | "sub"
            | "mul"
            | "divexact"
            | "gcd"
            | "neg"
            | "pow"
            | "derivative"
            | "eval"
            | "subst"
            | "resultant"
            | "discriminant"
            | "rat_add"
            | "rat_sub"
            | "rat_mul"
            | "rat_div"
            | "rat_sum"
            | "linear_factors"
            | "partial_fractions"
    )
    .then(|| evaluate_supported(request, op))
}

fn evaluate_supported(request: &Value, op: &str) -> Result<Value> {
    match op {
        "algebraic_letters_clear" => {
            algebraic_letters_clear()?;
            Ok(json!({"op": op, "size": 0}))
        }
        "algebraic_letters_show" => {
            let entries = algebraic_letters_show()?;
            Ok(json!({
                "op": op,
                "entries": entries.iter().map(algebraic_entry_value).collect::<Vec<_>>(),
                "size": entries.len(),
            }))
        }
        "algebraic_letters_allocate" => {
            let _session = join_algebraic_letter_session()?;
            let polynomial = string_field(request, "polynomial")?;
            let variable_name = string_field(request, "var")?;
            let ctx = algebraic_context(request, Some(variable_name))?;
            let variable = ctx
                .index_of(variable_name)
                .ok_or_else(|| Error::UnknownVariable(variable_name.to_owned()))?;
            let index = algebraic_letters_allocate(&parse_wire_poly(&ctx, polynomial)?, variable)?;
            let entry = algebraic_letters_show()?
                .into_iter()
                .find(|entry| entry.idx == index)
                .ok_or_else(|| {
                    Error::InvalidInput(format!(
                        "allocated algebraic-letter index {index} is unavailable"
                    ))
                })?;
            Ok(json!({
                "op": op,
                "idx": index,
                "sum": wire_rat(&entry.sum_value),
                "product": wire_rat(&entry.product_value),
                "discriminant": wire_poly(&entry.discriminant),
                "wm": crate::symbols::legacy::special_name_from_atom(entry.atoms().minus.as_view()),
                "wp": crate::symbols::legacy::special_name_from_atom(entry.atoms().plus.as_view()),
            }))
        }
        "simplify_with_vieta" | "back_substitute" | "combine_wm_wp_ratios" => {
            let _session = join_algebraic_letter_session()?;
            let expression = string_field(request, "expr")?;
            let ctx = algebraic_context(request, None)?;
            replay_algebraic_allocations(&ctx, request)?;
            let input = parse_wire_rat(&ctx, expression)?;
            let result = match op {
                "simplify_with_vieta" => simplify_with_vieta(&input)?,
                "back_substitute" => back_substitute(&input)?,
                "combine_wm_wp_ratios" => combine_wm_wp_ratios(&input)?,
                _ => unreachable!(),
            };
            Ok(result_response(op, &ctx, result))
        }
        "factor" => {
            let expression = string_field(request, "expr")?;
            let ctx = context_for(request, &[expression])?;
            let factorization = parse_wire_poly(&ctx, expression)?.factor();
            let factors = factorization
                .factors
                .into_iter()
                .map(|(factor, exponent)| json!([wire_poly(&factor), exponent]))
                .collect::<Vec<_>>();
            Ok(json!({
                "op": op,
                "constant": factorization.constant,
                "factors": factors,
                "vars": wire_context_variables(&ctx),
            }))
        }
        "add" | "sub" | "mul" | "divexact" | "gcd" => {
            let left = string_field(request, "a")?;
            let right = string_field(request, "b")?;
            let ctx = context_for(request, &[left, right])?;
            let left = parse_wire_poly(&ctx, left)?;
            let right = parse_wire_poly(&ctx, right)?;
            let result = match op {
                "add" => left.try_add(&right)?,
                "sub" => left.try_sub(&right)?,
                "mul" => left.try_mul(&right)?,
                "divexact" => left.div_exact(&right)?,
                "gcd" => left.gcd(&right)?,
                _ => unreachable!(),
            };
            Ok(result_response(op, &ctx, result))
        }
        "neg" => {
            let expression = string_field(request, "a")?;
            let ctx = context_for(request, &[expression])?;
            let result = -&parse_wire_poly(&ctx, expression)?;
            Ok(result_response(op, &ctx, result))
        }
        "pow" => {
            let expression = string_field(request, "a")?;
            let exponent = super::wire::integer_field(request, "n")?;
            if exponent < 0 {
                return Err(Error::InvalidExponent(exponent));
            }
            let ctx = context_for(request, &[expression])?;
            let result = parse_wire_poly(&ctx, expression)?.pow(exponent as usize);
            Ok(result_response(op, &ctx, result))
        }
        "derivative" => {
            let expression = string_field(request, "a")?;
            let ctx = context_for_variable(request, &[expression])?;
            let variable = variable_index(&ctx, request)?;
            let result = parse_wire_poly(&ctx, expression)?.derivative(variable)?;
            Ok(result_response(op, &ctx, result))
        }
        "eval" => {
            let expression = string_field(request, "a")?;
            let ctx = context_for(request, &[expression])?;
            let values = request
                .get("values")
                .and_then(Value::as_array)
                .ok_or_else(|| Error::InvalidInput("`values` must be an array".into()))?
                .iter()
                .map(|value| {
                    let value = value.as_str().ok_or_else(|| {
                        Error::InvalidInput("evaluation values must be strings".into())
                    })?;
                    parse_wire_rational_scalar(&ctx, value)
                })
                .collect::<Result<Vec<_>>>()?;
            let result = parse_wire_poly(&ctx, expression)?.evaluate_rational(&values)?;
            Ok(result_response(op, &ctx, result.to_string()))
        }
        "subst" => {
            let expression = string_field(request, "a")?;
            let value = string_field(request, "value")?;
            let ctx = context_for_variable(request, &[expression, value])?;
            let variable = variable_index(&ctx, request)?;
            let value = parse_wire_rational_scalar(&ctx, value)?;
            let result =
                parse_wire_poly(&ctx, expression)?.substitute_rational(variable, &value)?;
            Ok(result_response(op, &ctx, result))
        }
        "resultant" => {
            let left = string_field(request, "a")?;
            let right = string_field(request, "b")?;
            let ctx = context_for_variable(request, &[left, right])?;
            let variable = variable_index(&ctx, request)?;
            let result =
                parse_wire_poly(&ctx, left)?.resultant(&parse_wire_poly(&ctx, right)?, variable)?;
            Ok(result_response(op, &ctx, result))
        }
        "discriminant" => {
            let expression = string_field(request, "expr")?;
            let ctx = context_for_variable(request, &[expression])?;
            let variable = variable_index(&ctx, request)?;
            let result = parse_wire_poly(&ctx, expression)?.resultant_discriminant(variable)?;
            Ok(result_response(op, &ctx, result))
        }
        "rat_add" | "rat_sub" | "rat_mul" | "rat_div" => {
            let left = string_field(request, "a")?;
            let right = string_field(request, "b")?;
            let ctx = context_for(request, &[left, right])?;
            let left = parse_wire_rat(&ctx, left)?;
            let right = parse_wire_rat(&ctx, right)?;
            let result = match op {
                "rat_add" => left.try_add(&right)?,
                "rat_sub" => left.try_sub(&right)?,
                "rat_mul" => left.try_mul(&right)?,
                "rat_div" => left.try_div(&right)?,
                _ => unreachable!(),
            };
            Ok(result_response(op, &ctx, result))
        }
        "rat_sum" => {
            let terms = array_field(request, "terms")?;
            if terms.is_empty() {
                return Err(Error::InvalidInput(
                    "rat_sum requires a non-empty `terms` array".into(),
                ));
            }
            let mut expressions = Vec::with_capacity(terms.len() * 2);
            for term in terms {
                let pair = term.as_array().ok_or_else(|| {
                    Error::InvalidInput("rat_sum terms must be [numerator, denominator]".into())
                })?;
                if pair.len() != 2 {
                    return Err(Error::InvalidInput(
                        "rat_sum terms must have exactly two entries".into(),
                    ));
                }
                expressions.push(pair[0].as_str().ok_or_else(|| {
                    Error::InvalidInput("rat_sum numerator must be a string".into())
                })?);
                expressions.push(pair[1].as_str().ok_or_else(|| {
                    Error::InvalidInput("rat_sum denominator must be a string".into())
                })?);
            }
            let ctx = context_for(request, &expressions)?;
            let mut sum = Rat::zero(ctx.clone());
            for pair in expressions.chunks_exact(2) {
                let term = Rat::new(
                    parse_wire_poly(&ctx, pair[0])?,
                    parse_wire_poly(&ctx, pair[1])?,
                )?;
                sum = sum.try_add(&term)?;
            }
            Ok(result_response(op, &ctx, sum))
        }
        "linear_factors" => {
            let expression = string_field(request, "poly")?;
            let introduce_algebraic_letters = introduction_requested(request)?;
            let _session = introduce_algebraic_letters
                .then(begin_algebraic_letter_session)
                .transpose()?;
            let ctx = factor_context(request, &[expression], introduce_algebraic_letters)?;
            let variable = variable_index(&ctx, request)?;
            let factors = linear_factors_with_options(
                &parse_wire_poly(&ctx, expression)?,
                variable,
                &LinearFactorOptions {
                    introduce_algebraic_letters,
                    forbidden_variables: &[],
                },
            )?;
            let mut response = json!({
                "op": op,
                "constant": factors.constant,
                "linear": factors.linear.iter().map(|factor| json!([
                    factor.multiplicity,
                    wire_poly(factor.pole.numerator()),
                    wire_poly(factor.pole.denominator()),
                ])).collect::<Vec<_>>(),
                "nonlinear": factors.nonlinear.iter().map(|factor| json!([
                    factor.multiplicity,
                    wire_poly(&factor.polynomial),
                    factor.degree_in_var,
                ])).collect::<Vec<_>>(),
                "vars": wire_context_variables(&ctx),
            });
            if introduce_algebraic_letters {
                response.as_object_mut().expect("JSON object").insert(
                    "algebraic_letters".into(),
                    Value::Array(
                        algebraic_letters_show()?
                            .iter()
                            .map(algebraic_entry_value)
                            .collect(),
                    ),
                );
            }
            Ok(response)
        }
        "partial_fractions" => {
            let expression = string_field(request, "f")?;
            let introduce_algebraic_letters = introduction_requested(request)?;
            let _session = introduce_algebraic_letters
                .then(begin_algebraic_letter_session)
                .transpose()?;
            let ctx = factor_context(request, &[expression], introduce_algebraic_letters)?;
            let variable = variable_index(&ctx, request)?;
            let fractions = partial_fractions_with_options(
                &parse_wire_rat(&ctx, expression)?,
                variable,
                &PartialFractionOptions {
                    introduce_algebraic_letters,
                    forbidden_variables: &[],
                },
            )?;
            let mut response = json!({
                "op": op,
                "var": wire_context_variables(&ctx)[variable],
                "polynomial_part": wire_rat(&fractions.polynomial_part),
                "poles": fractions.poles.iter().map(|pole| json!({
                    "pole": wire_rat(&pole.pole),
                    "multiplicity": pole.multiplicity,
                    "coefs": pole.coefs.iter().map(wire_rat).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "vars": wire_context_variables(&ctx),
            });
            if introduce_algebraic_letters {
                response.as_object_mut().expect("JSON object").insert(
                    "algebraic_letters".into(),
                    Value::Array(
                        algebraic_letters_show()?
                            .iter()
                            .map(algebraic_entry_value)
                            .collect(),
                    ),
                );
            }
            Ok(response)
        }
        _ => unreachable!("operation was checked by algebra::evaluate"),
    }
}

fn introduction_requested(request: &Value) -> Result<bool> {
    if request.get("introduce_algebraic_letters").is_some() {
        optional_bool(request, "introduce_algebraic_letters", false)
    } else {
        optional_bool(request, "algebraic_letters", false)
    }
}

fn factor_context(
    request: &Value,
    expressions: &[&str],
    introduce_algebraic_letters: bool,
) -> Result<Arc<PolyCtx>> {
    let base = context_for_variable(request, expressions)?;
    if !introduce_algebraic_letters {
        return Ok(base);
    }
    let variables = (0..base.len())
        .map(|index| base.variable_atom(index))
        .collect::<Result<Vec<_>>>()?;
    PolyCtx::from_indeterminates(build_algebraic_letter_atom_list(
        variables,
        DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE,
    ))
}

fn algebraic_context(request: &Value, required_variable: Option<&str>) -> Result<Arc<PolyCtx>> {
    let mut variables = explicit_variables(request)?.unwrap_or_else(|| vec!["x".into()]);
    if variables.is_empty() {
        variables.push("x".into());
    }
    if let Some(variable) = required_variable
        && !variables.iter().any(|candidate| candidate == variable)
    {
        variables.push(variable.to_owned());
    }
    let variables = variables
        .iter()
        .map(|name| crate::symbols::legacy::atom_from_name(name))
        .collect::<Result<Vec<_>>>()?;
    PolyCtx::from_indeterminates(build_algebraic_letter_atom_list(
        variables,
        DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE,
    ))
}

fn replay_algebraic_allocations(ctx: &Arc<PolyCtx>, request: &Value) -> Result<()> {
    algebraic_letters_clear()?;
    let Some(allocations) = request.get("allocations") else {
        return Ok(());
    };
    for allocation in allocations
        .as_array()
        .ok_or_else(|| Error::InvalidInput("`allocations` must be an array".into()))?
    {
        let polynomial = parse_wire_poly(ctx, string_field(allocation, "polynomial")?)?;
        let variable = string_field(allocation, "var")?;
        let variable_atom = crate::symbols::legacy::atom_from_name(variable)?;
        let variable = ctx
            .index_of_indeterminate(variable_atom.as_view())
            .ok_or_else(|| Error::UnknownVariable(variable.to_owned()))?;
        algebraic_letters_allocate(&polynomial, variable)?;
    }
    Ok(())
}

pub(super) fn algebraic_entry_value(entry: &crate::algebra::AlgebraicLetterEntry) -> Value {
    let atoms = entry.atoms();
    json!({
        "idx": entry.idx,
        "polynomial": wire_poly(&entry.polynomial),
        "var_idx": entry.var_idx,
        "sum": wire_rat(&entry.sum_value),
        "product": wire_rat(&entry.product_value),
        "discriminant": wire_poly(&entry.discriminant),
        "wm": crate::symbols::legacy::special_name_from_atom(atoms.minus.as_view()),
        "wp": crate::symbols::legacy::special_name_from_atom(atoms.plus.as_view()),
        "wm_over_wp": crate::symbols::legacy::special_name_from_atom(atoms.ratio.as_view()),
    })
}
