//! Expression conversion, word algebra, transformations, and differentiation.

use std::sync::Arc;

use serde_json::{Value, json};

use super::wire::{
    context_for, context_for_variable, explicit_variables, integer_array_field,
    mzv_context_for_variable, optional_bool, parse_regulator, parse_wire_rat, parse_word,
    parse_wordlist, parse_words, regulator_sym_value, regulator_value, string_array_field,
    string_field, variable_index, wire_context_variables, wire_rat, wordlist_value,
};
use crate::algebra::convert::{
    convert_ab_to_zero_infinity, convert_one_infinity_to_zero_one, convert_zero_one,
};
use crate::algebra::diff::{diff_hlog, diff_mpl};
use crate::algebra::shuffle::{collect_words, concat_mul, shuffle_product, shuffle_words};
use crate::algebra::{
    DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE, begin_algebraic_letter_session,
    build_algebraic_letter_atom_list,
};
use crate::convert::{convert_to_hlog_reg_inf, parse_expression};
use crate::error::{Error, Result};
use crate::integrator::{
    TransformOptions, TransformResult, reg_head, reg_tail, reg0, reglim_word_with_table,
    regzero_word_in_ctx, shuffle_symbolic, transform_shuffle_with_options_and_table,
    transform_word_with_options_and_table,
};
use crate::reduce::MzvReductionTable;

pub(super) fn evaluate(request: &Value, op: &str) -> Option<Result<Value>> {
    matches!(
        op,
        "parse_expr"
            | "convert_to_hlog_reg_inf"
            | "shuffle_words"
            | "shuffle_product"
            | "concat_mul"
            | "collect_words"
            | "convert_zero_one"
            | "convert_1inf_to_01"
            | "regzero_word"
            | "reg0"
            | "reg_head"
            | "reg_tail"
            | "shuffle_symbolic"
            | "convert_ab_to_zero_inf"
            | "reglim_word"
            | "transform_word"
            | "transform_shuffle"
            | "diff_hlog"
            | "diff_mpl"
    )
    .then(|| evaluate_supported(request, op))
}

fn evaluate_supported(request: &Value, op: &str) -> Result<Value> {
    match op {
        "parse_expr" => {
            let expression = string_field(request, "expr")?;
            let user_variables = explicit_variables(request)?.unwrap_or_default();
            let parsed = parse_expression(expression, &user_variables, false)?;
            Ok(json!({
                "op": op,
                "canonical": parsed.expr.canonical_string(),
                "vars": parsed.augmented_vars,
            }))
        }
        "convert_to_hlog_reg_inf" => {
            let expression = string_field(request, "expr")?;
            let user_variables = explicit_variables(request)?.unwrap_or_default();
            let parsed = match parse_expression(expression, &user_variables, false) {
                Ok(parsed) => parsed,
                Err(error) => {
                    return Ok(json!({
                        "op": op,
                        "failed": true,
                        "reason": error.to_string(),
                        "vars": user_variables,
                    }));
                }
            };
            match convert_to_hlog_reg_inf(&parsed.expr, &parsed.ctx) {
                Ok(result) => Ok(json!({
                    "op": op,
                    "result": regulator_value(&result),
                    "vars": parsed.augmented_vars,
                })),
                Err(error) => Ok(json!({
                    "op": op,
                    "failed": true,
                    "reason": error.to_string(),
                    "vars": parsed.augmented_vars,
                })),
            }
        }
        "shuffle_words" => {
            let left_values = string_array_field(request, "v")?;
            let right_values = string_array_field(request, "w")?;
            let expressions = left_values
                .iter()
                .chain(&right_values)
                .map(String::as_str)
                .collect::<Vec<_>>();
            let ctx = context_for(request, &expressions)?;
            let left = parse_word(&ctx, &left_values)?;
            let right = parse_word(&ctx, &right_values)?;
            Ok(json!({
                "op": op,
                "result": wordlist_value(&shuffle_words(&left, &right)),
                "vars": wire_context_variables(&ctx),
            }))
        }
        "shuffle_product" | "concat_mul" => {
            let ctx = context_for(request, &[])?;
            let left = parse_wordlist(&ctx, request, "a")?;
            let right = parse_wordlist(&ctx, request, "b")?;
            let output = if op == "shuffle_product" {
                shuffle_product(&left, &right)
            } else {
                concat_mul(&left, &right)
            };
            Ok(
                json!({"op": op, "result": wordlist_value(&output), "vars": wire_context_variables(&ctx)}),
            )
        }
        "collect_words" | "convert_zero_one" | "convert_1inf_to_01" => {
            let ctx = context_for(request, &[])?;
            let wordlist = parse_wordlist(&ctx, request, "wl")?;
            let output = match op {
                "collect_words" => collect_words(&wordlist),
                "convert_zero_one" => convert_zero_one(&wordlist)?,
                "convert_1inf_to_01" => convert_one_infinity_to_zero_one(&wordlist)?,
                _ => unreachable!(),
            };
            Ok(
                json!({"op": op, "result": wordlist_value(&output), "vars": wire_context_variables(&ctx)}),
            )
        }
        "regzero_word" => {
            let letter_values = string_array_field(request, "word")?;
            let expressions = letter_values.iter().map(String::as_str).collect::<Vec<_>>();
            let ctx = context_for(request, &expressions)?;
            let word = parse_word(&ctx, &letter_values)?;
            let output = regzero_word_in_ctx(&ctx, &word)?;
            Ok(
                json!({"op": op, "result": wordlist_value(&output), "vars": wire_context_variables(&ctx)}),
            )
        }
        "reg0" => {
            let ctx = context_for(request, &[])?;
            let input = parse_wordlist(&ctx, request, "wl")?;
            let output = reg0(&input)?;
            Ok(
                json!({"op": op, "result": wordlist_value(&output), "vars": wire_context_variables(&ctx)}),
            )
        }
        "reg_head" | "reg_tail" => {
            let letter_expression = request.get("letter").and_then(Value::as_str).unwrap_or("0");
            let substitute_expression = request
                .get("substitute")
                .and_then(Value::as_str)
                .unwrap_or("0");
            let ctx = context_for(request, &[letter_expression, substitute_expression])?;
            let input = parse_wordlist(&ctx, request, "wl")?;
            let letter = parse_wire_rat(&ctx, letter_expression)?;
            let substitute = parse_wire_rat(&ctx, substitute_expression)?;
            let output = if op == "reg_head" {
                reg_head(&input, &letter, &substitute)?
            } else {
                reg_tail(&input, &letter, &substitute)?
            };
            Ok(
                json!({"op": op, "result": wordlist_value(&output), "vars": wire_context_variables(&ctx)}),
            )
        }
        "shuffle_symbolic" => {
            let ctx = context_for(request, &[])?;
            let left = parse_regulator(&ctx, request, "a")?;
            let right = parse_regulator(&ctx, request, "b")?;
            let output = shuffle_symbolic(&left, &right)?;
            Ok(
                json!({"op": op, "result": regulator_value(&output), "vars": wire_context_variables(&ctx)}),
            )
        }
        "convert_ab_to_zero_inf" => {
            let a_expression = string_field(request, "A")?;
            let b_expression = string_field(request, "B")?;
            let ctx = context_for(request, &[a_expression, b_expression])?;
            let wordlist = parse_wordlist(&ctx, request, "wl")?;
            let a = parse_wire_rat(&ctx, a_expression)?;
            let b = parse_wire_rat(&ctx, b_expression)?;
            let output = convert_ab_to_zero_infinity(&wordlist, &a, &b)?;
            Ok(
                json!({"op": op, "result": wordlist_value(&output), "vars": wire_context_variables(&ctx)}),
            )
        }
        "reglim_word" | "transform_word" => {
            let letter_values = string_array_field(request, "word")?;
            let expressions = letter_values.iter().map(String::as_str).collect::<Vec<_>>();
            let introduce_algebraic_letters =
                op == "transform_word" && optional_bool(request, "algebraic_letters", false)?;
            let (ctx, response_variables, table) =
                transform_context(request, &expressions, introduce_algebraic_letters)?;
            let variable = variable_index(&ctx, request)?;
            let word = parse_word(&ctx, &letter_values)?;
            if op == "reglim_word" {
                let output = reglim_word_with_table(&ctx, &word, variable, &table)?;
                return Ok(json!({
                    "op": op,
                    "result": regulator_sym_value(&output),
                    "vars": response_variables,
                }));
            }
            let _session = introduce_algebraic_letters
                .then(begin_algebraic_letter_session)
                .transpose()?;
            let options = TransformOptions {
                introduce_algebraic_letters,
                forbidden_variables: &[],
            };
            match transform_word_with_options_and_table(
                &ctx,
                &word,
                variable,
                &options,
                Some(&table),
            ) {
                Ok(output) => Ok(json!({
                    "op": op,
                    "result": transform_result_value(&output),
                    "vars": response_variables,
                })),
                Err(error) if error.to_string().contains("$Failed") => Ok(json!({
                    "op": op,
                    "failed": true,
                    "reason": error.to_string(),
                    "vars": response_variables,
                })),
                Err(error) => Err(error),
            }
        }
        "transform_shuffle" => {
            let words_value = request
                .get("wordlist")
                .ok_or_else(|| Error::InvalidInput("missing array field `wordlist`".into()))?;
            let introduce_algebraic_letters = optional_bool(request, "algebraic_letters", false)?;
            let (ctx, response_variables, table) =
                transform_context(request, &[], introduce_algebraic_letters)?;
            let variable = variable_index(&ctx, request)?;
            let words = parse_words(&ctx, words_value)?;
            let _session = introduce_algebraic_letters
                .then(begin_algebraic_letter_session)
                .transpose()?;
            let options = TransformOptions {
                introduce_algebraic_letters,
                forbidden_variables: &[],
            };
            match transform_shuffle_with_options_and_table(&ctx, &words, variable, &options, &table)
            {
                Ok(output) => Ok(json!({
                    "op": op,
                    "result": transform_result_value(&output),
                    "vars": response_variables,
                })),
                Err(error) if error.to_string().contains("$Failed") => Ok(json!({
                    "op": op,
                    "failed": true,
                    "reason": error.to_string(),
                    "vars": response_variables,
                })),
                Err(error) => Err(error),
            }
        }
        "diff_hlog" => {
            let z_expression = string_field(request, "z")?;
            let letter_values = string_array_field(request, "word")?;
            let mut expressions = vec![z_expression];
            expressions.extend(letter_values.iter().map(String::as_str));
            let ctx = context_for_variable(request, &expressions)?;
            let variable = variable_index(&ctx, request)?;
            let z = parse_wire_rat(&ctx, z_expression)?;
            let word = parse_word(&ctx, &letter_values)?;
            let terms = diff_hlog(&z, &word, variable)?;
            let mut total = Vec::with_capacity(terms.len());
            let values = terms
                .iter()
                .map(|term| {
                    let mut summand = format!("({})", wire_rat(&term.coef));
                    if !term.hlog.word.is_empty() {
                        let args = std::iter::once(wire_rat(&term.hlog.z))
                            .chain(term.hlog.word.letters.iter().map(wire_rat))
                            .collect::<Vec<_>>()
                            .join(",");
                        summand.push_str(&format!("*H({args})"));
                    }
                    total.push(summand);
                    json!({
                        "coef": wire_rat(&term.coef),
                        "z": wire_rat(&term.hlog.z),
                        "word": term.hlog.word.letters.iter().map(wire_rat).collect::<Vec<_>>(),
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({
                "op": op,
                "result": values,
                "total_str": if total.is_empty() { String::from("0") } else { total.join(" + ") },
                "vars": wire_context_variables(&ctx),
            }))
        }
        "diff_mpl" => {
            let indices = integer_array_field(request, "ns")?;
            let argument_values = string_array_field(request, "zs")?;
            let expressions = argument_values
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let ctx = context_for_variable(request, &expressions)?;
            let variable = variable_index(&ctx, request)?;
            let arguments = argument_values
                .iter()
                .map(|value| parse_wire_rat(&ctx, value))
                .collect::<Result<Vec<_>>>()?;
            let terms = diff_mpl(&indices, &arguments, variable)?;
            let mut total = Vec::with_capacity(terms.len());
            let values = terms
                .iter()
                .map(|term| {
                    let mut summand = format!("({})", wire_rat(&term.coef));
                    if !term.mpl.indices.is_empty() {
                        let suffix = term
                            .mpl
                            .indices
                            .iter()
                            .map(|index| {
                                if *index < 0 {
                                    format!("_m{}", index.unsigned_abs())
                                } else {
                                    format!("_{index}")
                                }
                            })
                            .collect::<String>();
                        let args = term
                            .mpl
                            .args
                            .iter()
                            .map(wire_rat)
                            .collect::<Vec<_>>()
                            .join(",");
                        summand.push_str(&format!("*M{suffix}({args})"));
                    }
                    total.push(summand);
                    json!({
                        "coef": wire_rat(&term.coef),
                        "ns": &term.mpl.indices,
                        "zs": term.mpl.args.iter().map(wire_rat).collect::<Vec<_>>(),
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({
                "op": op,
                "result": values,
                "total_str": if total.is_empty() { String::from("0") } else { total.join(" + ") },
                "vars": wire_context_variables(&ctx),
            }))
        }
        _ => unreachable!("operation was checked by words::evaluate"),
    }
}

fn transform_context(
    request: &Value,
    expressions: &[&str],
    introduce_algebraic_letters: bool,
) -> Result<(Arc<crate::core::PolyCtx>, Vec<String>, MzvReductionTable)> {
    let (base, table) = mzv_context_for_variable(request, expressions)?;
    let response_variables = wire_context_variables(&base);
    if !introduce_algebraic_letters {
        return Ok((base, response_variables, table));
    }
    let variables = (0..base.len())
        .map(|index| base.variable_atom(index))
        .collect::<Result<Vec<_>>>()?;
    let ctx = crate::core::PolyCtx::from_indeterminates(build_algebraic_letter_atom_list(
        variables,
        DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE,
    ))?;
    Ok((ctx, response_variables, table))
}

fn transform_result_value(result: &TransformResult) -> Value {
    Value::Array(
        result
            .iter()
            .map(|pair| {
                json!({
                    "shuffle": wordlist_value(&pair.shuffle),
                    "regulator": regulator_sym_value(&pair.regulator),
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_word_honors_the_algebraic_letter_option_without_leaking_pool_vars() {
        let base = json!({
            "op": "transform_word",
            "word": ["bridge_transform_x^2+1"],
            "var": "bridge_transform_x",
            "vars": ["bridge_transform_x"],
        });
        let strict = evaluate_supported(&base, "transform_word").unwrap();
        assert!(!strict.to_string().contains("Wm_"));
        assert!(!strict.to_string().contains("Wp_"));

        let mut algebraic = base;
        algebraic["algebraic_letters"] = Value::Bool(true);
        let transformed = evaluate_supported(&algebraic, "transform_word").unwrap();
        let serialized = transformed.to_string();
        assert!(serialized.contains("Wm_1"), "{serialized}");
        assert!(serialized.contains("Wp_1"), "{serialized}");
        assert_eq!(transformed["vars"], json!(["bridge_transform_x"]));
    }
}
