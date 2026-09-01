//! Integration-step and end-to-end HyperInt bridge operations.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use serde_json::{Value, json};

use super::algebra::algebraic_entry_value;
use super::mzv_data::mzv_reduction_table;
use super::narrow::{mzv_indeterminates, payload_strings};
use super::wire::{
    array_field, explicit_variables, optional_bool, parse_wire_rat, parse_wordlist, parse_words,
    regulator_sym_value, scan_identifiers, string_array_field, string_field, variable_index,
    wire_context_variables, wordlist_value,
};
use crate::algebra::algebraic_letters::{
    DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE, algebraic_letters_show, begin_algebraic_letter_session,
    build_algebraic_letter_atom_list,
};
use crate::convert::{convert_to_hlog_reg_inf, parse_expression};
use crate::core::PolyCtx;
use crate::error::{Error, Result};
use crate::integrator::{
    HyperIntOptions, IntegrateIiOptions, IntegrationError, IntegrationStepOptions, ShuffleEntry,
    ShuffleList, differentiate_wordlist, hyperflint_with_options_and_spectators,
    integrate_ii_with_options, integration_step_with_options_and_remaining_variables,
    rescale_interval_with_bound_parser,
};
use crate::reduce::{MzvReductionTable, build_mzv_var_list, build_narrow_var_list};
use crate::symbols::is_library_constant;

pub(super) fn evaluate(request: &Value, op: &str) -> Option<Result<Value>> {
    matches!(
        op,
        "differentiate_wordlist" | "integrate_ii" | "integration_step" | "hyperflint"
    )
    .then(|| evaluate_supported(request, op))
}

fn evaluate_supported(request: &Value, op: &str) -> Result<Value> {
    match op {
        "differentiate_wordlist" => {
            let ctx = super::wire::context_for_variable(request, &[])?;
            let input = parse_wordlist(&ctx, request, "wl")?;
            let output = differentiate_wordlist(&input, variable_index(&ctx, request)?)?;
            Ok(
                json!({"op": op, "result": wordlist_value(&output), "vars": wire_context_variables(&ctx)}),
            )
        }
        "integrate_ii" => {
            let variable_name = string_field(request, "var")?;
            let introduce_algebraic_letters = optional_bool(request, "algebraic_letters", false)?;
            let _algebraic_session = introduce_algebraic_letters
                .then(begin_algebraic_letter_session)
                .transpose()?;
            let mut variables = explicit_variables(request)?.unwrap_or_default();
            if !variables.iter().any(|candidate| candidate == variable_name) {
                variables.push(variable_name.to_owned());
            }
            let variables = variables
                .iter()
                .map(|name| crate::symbols::legacy::atom_from_name(name))
                .collect::<Result<Vec<_>>>()?;
            let variables = if introduce_algebraic_letters {
                build_algebraic_letter_atom_list(variables, DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE)
            } else {
                variables
            };
            let ctx = PolyCtx::from_indeterminates(variables)?;
            let input = parse_wordlist(&ctx, request, "wl")?;
            let variable = variable_index(&ctx, request)?;
            match integrate_ii_with_options(
                &ctx,
                &input,
                variable,
                &IntegrateIiOptions {
                    introduce_algebraic_letters,
                    forbidden_variables: &[],
                },
            ) {
                Ok(output) => Ok(json!({
                    "op": op,
                    "result": wordlist_value(&output),
                    "vars": wire_context_variables(&ctx),
                })),
                Err(error) => Ok(json!({
                    "op": op,
                    "failed": true,
                    "reason": error.to_string(),
                    "vars": wire_context_variables(&ctx),
                })),
            }
        }
        "integration_step" => {
            let variable_name = string_field(request, "var")?.to_owned();
            let remaining_variable_names = request
                .get("remaining_vars")
                .map(|_| string_array_field(request, "remaining_vars"))
                .transpose()?
                .unwrap_or_default();
            let introduce_algebraic_letters = optional_bool(request, "algebraic_letters", false)?;
            let _algebraic_session = introduce_algebraic_letters
                .then(begin_algebraic_letter_session)
                .transpose()?;
            let mut required_variables = vec![variable_name.clone()];
            for remaining in &remaining_variable_names {
                if !required_variables.contains(remaining) {
                    required_variables.push(remaining.clone());
                }
            }
            let payload_expressions = payload_strings(request, &["wordlist"]);
            let (ctx, table) = integration_context(
                request,
                &required_variables,
                &payload_expressions,
                introduce_algebraic_letters,
            )?;
            let variable = variable_index(&ctx, request)?;
            let remaining_variables = remaining_variable_names
                .iter()
                .map(|name| {
                    let atom = crate::symbols::legacy::atom_from_name(name)?;
                    ctx.index_of_indeterminate(atom.as_view())
                        .ok_or_else(|| Error::UnknownVariable(name.clone()))
                })
                .collect::<Result<Vec<_>>>()?;
            let input = parse_shuffle_list(&ctx, request, "wordlist")?;
            let options = IntegrationStepOptions {
                check_divergences: optional_bool(request, "check_divergences", false)?,
                parallel: optional_bool(request, "parallel", true)?,
                introduce_algebraic_letters,
            };
            match integration_step_with_options_and_remaining_variables(
                &ctx,
                &input,
                variable,
                &table,
                &options,
                &remaining_variables,
            ) {
                Ok(output) => Ok(json!({
                    "op": op,
                    "result": regulator_sym_value(&output),
                    "vars": wire_context_variables(&ctx),
                })),
                Err(error @ IntegrationError::Divergent { .. }) => Ok(json!({
                    "op": op,
                    "divergent": true,
                    "reason": error.to_string(),
                    "vars": wire_context_variables(&ctx),
                })),
                Err(error) => Ok(json!({
                    "op": op,
                    "failed": true,
                    "reason": error.to_string(),
                    "vars": wire_context_variables(&ctx),
                })),
            }
        }
        "hyperflint" => {
            let variables_to_integrate = request
                .get("vars_int")
                .map(|_| string_array_field(request, "vars_int"))
                .transpose()?
                .unwrap_or_default();
            let introduce_algebraic_letters = optional_bool(request, "algebraic_letters", false)?;
            let _algebraic_session = introduce_algebraic_letters
                .then(begin_algebraic_letter_session)
                .transpose()?;

            let expression_input = request.get("expr").and_then(Value::as_str);
            let rational_input = request.get("f").and_then(Value::as_str);
            let mut context_expressions = if let Some(expression) = expression_input {
                vec![expression]
            } else if let Some(expression) = rational_input {
                vec![expression]
            } else {
                payload_strings(request, &["wordlist"])
            };
            context_expressions.extend(payload_strings(request, &["vars_int_from", "vars_int_to"]));
            let mut base_variables = explicit_variables(request)?.unwrap_or_default();
            if base_variables.is_empty() {
                base_variables.extend(variables_to_integrate.iter().cloned());
            }
            for variable in &variables_to_integrate {
                if !base_variables.contains(variable) {
                    base_variables.push(variable.clone());
                }
            }
            if base_variables.is_empty() {
                base_variables.push("x".into());
            }
            let (ctx, table, mut input) = if let Some(expression) = expression_input {
                let table = mzv_reduction_table(request)?;
                let variables = if table.is_embedded_standard() {
                    build_narrow_var_list(&table, &base_variables, &context_expressions.join("+"))
                } else {
                    build_mzv_var_list(&table, base_variables.clone())
                };
                let variables = if introduce_algebraic_letters {
                    crate::symbols::legacy::append_algebraic_names(
                        variables,
                        DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE,
                    )
                } else {
                    variables
                };
                let parsed = match parse_expression(expression, &variables, false) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        return Ok(json!({
                            "op": op,
                            "failed": true,
                            "reason": error.to_string(),
                            "vars": variables,
                        }));
                    }
                };
                let regulator = match convert_to_hlog_reg_inf(&parsed.expr, &parsed.ctx) {
                    Ok(regulator) => regulator,
                    Err(error) => {
                        return Ok(json!({
                            "op": op,
                            "failed": true,
                            "reason": error.to_string(),
                            "vars": parsed.augmented_vars,
                        }));
                    }
                };
                let input = regulator
                    .into_iter()
                    .map(|term| ShuffleEntry::new(term.coef, term.key))
                    .collect();
                (parsed.ctx, table, input)
            } else {
                let (ctx, table) = integration_context(
                    request,
                    &variables_to_integrate,
                    &context_expressions,
                    introduce_algebraic_letters,
                )?;
                let input = if let Some(expression) = rational_input {
                    vec![ShuffleEntry::new(
                        parse_wire_rat(&ctx, expression)?,
                        Vec::new(),
                    )]
                } else {
                    parse_shuffle_list(&ctx, request, "wordlist")?
                };
                (ctx, table, input)
            };

            let variable_indices = variables_to_integrate
                .iter()
                .map(|variable| {
                    crate::symbols::legacy::atom_from_name(variable)
                        .ok()
                        .and_then(|atom| ctx.index_of_indeterminate(atom.as_view()))
                        .ok_or_else(|| Error::UnknownVariable(variable.clone()))
                })
                .collect::<Result<Vec<_>>>()?;
            let spectator_indices = hyperflint_spectator_indices(
                &ctx,
                &variable_indices,
                &table,
                introduce_algebraic_letters,
            )?;
            let from = request
                .get("vars_int_from")
                .map(|_| string_array_field(request, "vars_int_from"))
                .transpose()?
                .unwrap_or_default();
            let to = request
                .get("vars_int_to")
                .map(|_| string_array_field(request, "vars_int_to"))
                .transpose()?
                .unwrap_or_default();
            if !from.is_empty() || !to.is_empty() {
                if from.len() != variable_indices.len() || to.len() != variable_indices.len() {
                    return Err(Error::InvalidInput(
                        "vars_int_from/vars_int_to length mismatch with vars_int".into(),
                    ));
                }
                for ((&variable, from), to) in variable_indices.iter().zip(&from).zip(&to) {
                    input = rescale_interval_with_bound_parser(
                        &ctx,
                        &input,
                        variable,
                        from,
                        to,
                        |expression| parse_wire_rat(&ctx, expression),
                    )
                    .map_err(|error| Error::InvalidInput(error.to_string()))?;
                    if input.is_empty() {
                        break;
                    }
                }
            }

            let options = HyperIntOptions {
                step: IntegrationStepOptions {
                    check_divergences: optional_bool(request, "check_divergences", false)?,
                    parallel: optional_bool(request, "parallel", true)?,
                    introduce_algebraic_letters,
                },
                close_final_positive_letters: optional_bool(
                    request,
                    "close_positive_letters",
                    true,
                )?,
            };
            let started = Instant::now();
            match hyperflint_with_options_and_spectators(
                &ctx,
                &input,
                &variable_indices,
                &table,
                &options,
                &spectator_indices,
            ) {
                Ok(output) => {
                    let mut response = json!({
                        "op": op,
                        "result": regulator_sym_value(&output),
                        "timing_compute_s": started.elapsed().as_secs_f64(),
                        "vars": wire_context_variables(&ctx),
                    });
                    if introduce_algebraic_letters {
                        response.as_object_mut().expect("response is a map").insert(
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
                Err(error @ IntegrationError::Divergent { .. }) => Ok(json!({
                    "op": op,
                    "divergent": true,
                    "reason": error.to_string(),
                    "vars": wire_context_variables(&ctx),
                })),
                Err(error) => Ok(json!({
                    "op": op,
                    "failed": true,
                    "reason": error.to_string(),
                    "vars": wire_context_variables(&ctx),
                })),
            }
        }
        _ => unreachable!("operation was checked by integration::evaluate"),
    }
}

fn parse_shuffle_list(ctx: &Arc<PolyCtx>, request: &Value, name: &str) -> Result<ShuffleList> {
    array_field(request, name)?
        .iter()
        .map(|entry| {
            Ok(ShuffleEntry::new(
                parse_wire_rat(ctx, string_field(entry, "coef")?)?,
                parse_words(
                    ctx,
                    entry.get("shuffle").ok_or_else(|| {
                        Error::InvalidInput("shuffle-list entry is missing `shuffle`".into())
                    })?,
                )?,
            ))
        })
        .collect()
}

fn hyperflint_spectator_indices(
    ctx: &PolyCtx,
    integration_indices: &[usize],
    _table: &MzvReductionTable,
    _algebraic_letters: bool,
) -> Result<Vec<usize>> {
    // Classify only slots already present in the narrow context. Registered
    // constants are recognized by their central Symbolica heads, not by
    // membership in one particular reduction table or algebraic pool size.
    let reserved_indices = (0..ctx.len())
        .filter(|&index| {
            ctx.variable_atom(index)
                .is_ok_and(|atom| is_library_constant(atom.as_view()))
        })
        .collect::<HashSet<_>>();
    let integration_indices = integration_indices.iter().copied().collect::<HashSet<_>>();
    Ok((0..ctx.len())
        .filter(|index| !integration_indices.contains(index) && !reserved_indices.contains(index))
        .collect())
}

pub(super) fn integration_context(
    request: &Value,
    required_variables: &[String],
    expressions: &[&str],
    algebraic_letters: bool,
) -> Result<(Arc<PolyCtx>, MzvReductionTable)> {
    let table = mzv_reduction_table(request)?;
    let mut user_variables =
        explicit_variables(request)?.unwrap_or_else(|| scan_identifiers(expressions));
    for variable in required_variables {
        if !user_variables.iter().any(|candidate| candidate == variable) {
            user_variables.push(variable.clone());
        }
    }
    if user_variables.is_empty() {
        user_variables.push("x".into());
    }
    let user_atoms = user_variables
        .iter()
        .map(|name| crate::symbols::legacy::atom_from_name(name))
        .collect::<Result<Vec<_>>>()?;
    let variables = mzv_indeterminates(&table, user_atoms, expressions)?;
    let variables = if algebraic_letters {
        build_algebraic_letter_atom_list(variables, DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE)
    } else {
        variables
    };
    Ok((PolyCtx::from_indeterminates(variables)?, table))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use symbolica::prelude::symbol;

    use super::*;
    use crate::reduce::{mzv_constant_atom, standard_mzv_reductions};
    use crate::symbols::{algebraic_atoms, mzv_atom};

    #[test]
    fn standard_rule_lhs_is_reserved_without_widening_the_context() {
        let x = symbol!("bridge_spectator_x").to_atom();
        let y = symbol!("bridge_spectator_y").to_atom();
        let lhs = mzv_constant_atom("mzv_4").unwrap();
        let ctx = PolyCtx::from_indeterminates([x, y, lhs]).unwrap();
        assert_eq!(ctx.len(), 3);

        let spectators =
            hyperflint_spectator_indices(&ctx, &[0], &standard_mzv_reductions(), false).unwrap();
        assert_eq!(spectators, [1]);
        assert_eq!(ctx.len(), 3);
    }

    #[test]
    fn interval_bounds_participate_in_standard_mzv_narrowing() {
        let request = json!({
            "vars": ["x"],
            "vars_int_from": ["mzv_4"],
            "vars_int_to": ["1"],
        });
        let expressions = payload_strings(&request, &["vars_int_from", "vars_int_to"]);
        let (ctx, _) = integration_context(&request, &["x".into()], &expressions, false).unwrap();
        assert!(
            ctx.index_of_indeterminate(mzv_constant_atom("mzv_4").unwrap().as_view())
                .is_some()
        );
    }

    #[test]
    fn every_registered_mzv_and_algebraic_atom_is_reserved_structurally() {
        let x = symbol!("bridge_structural_spectator_x").to_atom();
        let mzv = mzv_atom(&[999]);
        let algebraic = algebraic_atoms(71).minus;
        let ctx = PolyCtx::from_indeterminates([x, mzv, algebraic]).unwrap();
        let spectators =
            hyperflint_spectator_indices(&ctx, &[0], &standard_mzv_reductions(), false).unwrap();
        assert!(spectators.is_empty());
    }
}
