//! MZV, contour, period, and fibration-basis protocol operations.

use serde_json::{Value, json};

use super::integration::integration_context;
use super::narrow::payload_strings;
use super::symcoef::symcoef_string;
use super::wire::{
    WireValue, array_field, mzv_context, optional_bool, parse_regulator, parse_wire_rat,
    parse_word, parse_wordlist, regulator_sym_value, regulator_value, result_response,
    string_array_field, string_field, unique_diagnostic_variable_index, wire_context_variables,
    wire_regkey,
};
use crate::core::SymCoef;
use crate::error::{Error, Result};
use crate::integrator::{RegKey, RegTermSym, regkey_structural_cmp};
use crate::reduce::{
    OnAxisEntry, OnAxisSymEntry, apply_mzv_reductions, break_up_contour, break_up_contour_sym,
    evaluate_periods, fibration_basis, fibration_basis_sym, test_zero_function, to_wordlist_sym,
    zero_inf_period, zero_one_period,
};

pub(super) fn evaluate(request: &Value, op: &str) -> Option<Result<Value>> {
    matches!(
        op,
        "apply_mzv_reductions"
            | "zero_one_period"
            | "zero_inf_period"
            | "evaluate_periods"
            | "test_zero_function"
            | "fibration_basis"
            | "break_up_contour"
            | "break_up_contour_sym"
    )
    .then(|| evaluate_supported(request, op))
}

fn evaluate_supported(request: &Value, op: &str) -> Result<Value> {
    match op {
        "apply_mzv_reductions" => {
            let expression = string_field(request, "f")?;
            let (ctx, table) = mzv_context(request, &[expression])?;
            let output = apply_mzv_reductions(&table, &parse_wire_rat(&ctx, expression)?)?;
            Ok(result_response(op, &ctx, output))
        }
        "zero_one_period" | "zero_inf_period" => {
            let letter_values = string_array_field(request, "word")?;
            let expressions = letter_values.iter().map(String::as_str).collect::<Vec<_>>();
            let (ctx, table) = mzv_context(request, &expressions)?;
            let word = parse_word(&ctx, &letter_values)?;
            let output = if op == "zero_one_period" {
                zero_one_period(&ctx, &word, &table)?
            } else {
                zero_inf_period(&ctx, &word, &table)?
            };
            Ok(result_response(op, &ctx, output))
        }
        "evaluate_periods" | "test_zero_function" => {
            let expressions = payload_strings(request, &["regulator"]);
            let (ctx, table) = mzv_context(request, &expressions)?;
            let regulator = parse_regulator(&ctx, request, "regulator")?;
            if op == "evaluate_periods" {
                let output = evaluate_periods(&ctx, &regulator, &table)?;
                Ok(json!({
                    "op": op,
                    "result": regulator_value(&output),
                    "vars": wire_context_variables(&ctx),
                }))
            } else {
                Ok(result_response(
                    op,
                    &ctx,
                    test_zero_function(&ctx, &regulator, &table)?,
                ))
            }
        }
        "fibration_basis" => {
            let variables_to_reduce = request
                .get("vars_int")
                .map(|_| string_array_field(request, "vars_int"))
                .transpose()?
                .unwrap_or_default();
            let expressions = payload_strings(request, &["wordlist"]);
            let (ctx, table) =
                integration_context(request, &variables_to_reduce, &expressions, false)?;
            let input = parse_regulator(&ctx, request, "wordlist")?;
            let variable_indices = variables_to_reduce
                .iter()
                .map(|variable| {
                    crate::symbols::legacy::atom_from_name(variable)
                        .ok()
                        .and_then(|atom| ctx.index_of_indeterminate(atom.as_view()))
                        .ok_or_else(|| Error::UnknownVariable(variable.clone()))
                })
                .collect::<Result<Vec<_>>>()?;
            if optional_bool(request, "use_sym", false)? {
                let symbolic = input
                    .into_iter()
                    .map(|term| RegTermSym {
                        coef: SymCoef::from_rat(&term.coef),
                        key: term.key,
                    })
                    .collect::<Vec<_>>();
                return match fibration_basis_sym(&ctx, &symbolic, &variable_indices, &table) {
                    Ok(result) => Ok(json!({
                        "op": op,
                        "variant": "sym",
                        "vars_int": result.vars,
                        "terms": fibration_terms_for_wire(&result.terms).into_iter().map(|(key, coefficient)| {
                            fibration_term_value(key, symcoef_string(coefficient))
                        }).collect::<Vec<_>>(),
                        "vars": wire_context_variables(&ctx),
                    })),
                    Err(error) => Ok(json!({
                        "op": op,
                        "variant": "sym",
                        "failed": true,
                        "reason": error.to_string(),
                        "vars": wire_context_variables(&ctx),
                    })),
                };
            }
            match fibration_basis(&ctx, &input, &variable_indices, &table) {
                Ok(result) => Ok(json!({
                    "op": op,
                    "vars_int": result.vars,
                    "terms": fibration_terms_for_wire(&result.terms).into_iter().map(|(key, coefficient)| {
                        fibration_term_value(key, coefficient)
                    }).collect::<Vec<_>>(),
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
        "break_up_contour" | "break_up_contour_sym" => {
            let expressions = payload_strings(request, &["wl", "on_axis"]);
            let (ctx, table) = mzv_context(request, &expressions)?;
            let wordlist = parse_wordlist(&ctx, request, "wl")?;
            let on_axis_values = array_field(request, "on_axis")?;
            if op == "break_up_contour" {
                if !on_axis_values.is_empty() {
                    return Err(Error::InvalidInput(
                        "non-empty `on_axis` requires break_up_contour_sym".into(),
                    ));
                }
                let output = break_up_contour(&ctx, &wordlist, &[] as &[OnAxisEntry], &table)?;
                return Ok(json!({
                    "op": op,
                    "result": regulator_value(&output),
                    "vars": wire_context_variables(&ctx),
                }));
            }

            let on_axis = on_axis_values
                .iter()
                .map(|entry| {
                    let letter = parse_wire_rat(&ctx, string_field(entry, "letter")?)?;
                    let imaginary_variable = string_field(entry, "im_var")?;
                    Ok(OnAxisSymEntry {
                        letter,
                        im_part: SymCoef::delta_factor(
                            ctx.clone(),
                            unique_diagnostic_variable_index(&ctx, imaginary_variable)?,
                        )?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let output = break_up_contour_sym(&ctx, &to_wordlist_sym(&wordlist), &on_axis, &table)?;
            Ok(json!({
                "op": op,
                "result": regulator_sym_value(&output),
                "vars": wire_context_variables(&ctx),
            }))
        }
        _ => unreachable!("operation was checked by reduction::evaluate"),
    }
}

fn fibration_terms_for_wire<T>(terms: &[(RegKey, T)]) -> Vec<(Vec<Vec<String>>, &T)> {
    let mut decorated = terms
        .iter()
        .map(|(key, coefficient)| (wire_regkey(key), key, coefficient))
        .collect::<Vec<_>>();
    decorated.sort_unstable_by(|(left_wire, left_key, _), (right_wire, right_key, _)| {
        left_wire
            .cmp(right_wire)
            .then_with(|| regkey_structural_cmp(left_key, right_key))
    });
    decorated
        .into_iter()
        .map(|(wire, _, coefficient)| (wire, coefficient))
        .collect()
}

fn fibration_term_value(key: Vec<Vec<String>>, coefficient: impl WireValue) -> Value {
    json!({
        "key": key,
        "coef": coefficient.wire_value(),
    })
}
