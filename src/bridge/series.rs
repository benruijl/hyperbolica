//! Series-expansion and finite multiple-polylogarithm sum operations.

use serde_json::{Value, json};

use super::wire::{
    context_for, context_for_variable, integer_array_field, integer_field, parse_wire_rat,
    parse_word, result_response, string_array_field, string_field, variable_index,
    wire_context_variables, wire_rat,
};
use crate::core::Rat;
use crate::error::{Error, Result};
use crate::series::expansions::{
    SeriesTable, expand_infinity_word_in_context, expand_zero_word_in_context,
};
use crate::series::hlog_series::{
    ExpansionSeries, HlogSeriesBranch, hlog_series, hlog_zero_expand,
};
use crate::series::laurent::series_expansion;
use crate::series::mpl_series::{MplSeriesBranch, mpl_series};
use crate::series::mpl_sum::mpl_sum;

pub(super) fn evaluate(request: &Value, op: &str) -> Option<Result<Value>> {
    matches!(
        op,
        "expand_zero_word"
            | "expand_inf_word"
            | "mpl_sum"
            | "hlog_zero_expand"
            | "hlog_series"
            | "mpl_series"
            | "pole_degree"
            | "rat_residue"
            | "series_expansion"
    )
    .then(|| evaluate_supported(request, op))
}

fn evaluate_supported(request: &Value, op: &str) -> Result<Value> {
    match op {
        "expand_zero_word" | "expand_inf_word" => {
            let letter_values = string_array_field(request, "word")?;
            let expressions = letter_values.iter().map(String::as_str).collect::<Vec<_>>();
            let ctx = context_for(request, &expressions)?;
            let word = parse_word(&ctx, &letter_values)?;
            let order = integer_field(request, "min_order")?;
            let table = if op == "expand_zero_word" {
                expand_zero_word_in_context(ctx.clone(), &word, order)?
            } else {
                expand_infinity_word_in_context(ctx.clone(), &word, order)?
            };
            Ok(
                json!({"op": op, "table": series_table_value(&table), "vars": wire_context_variables(&ctx)}),
            )
        }
        "mpl_sum" => {
            let indices = integer_array_field(request, "ns")?;
            let argument_values = string_array_field(request, "zs")?;
            let expressions = argument_values
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let ctx = context_for(request, &expressions)?;
            let arguments = argument_values
                .iter()
                .map(|value| parse_wire_rat(&ctx, value))
                .collect::<Result<Vec<_>>>()?;
            let sum = if arguments.is_empty() {
                if indices.is_empty() {
                    Rat::one(ctx.clone())
                } else {
                    Rat::zero(ctx.clone())
                }
            } else {
                mpl_sum(&indices, &arguments, integer_field(request, "max_n")?)?
            };
            Ok(result_response(op, &ctx, sum))
        }
        "hlog_zero_expand" | "hlog_series" => {
            let arg_expression = string_field(request, "arg")?;
            let letter_values = string_array_field(request, "word")?;
            let mut expressions = vec![arg_expression];
            expressions.extend(letter_values.iter().map(String::as_str));
            let ctx = if op == "hlog_series" {
                context_for_variable(request, &expressions)?
            } else {
                context_for(request, &expressions)?
            };
            let arg = parse_wire_rat(&ctx, arg_expression)?;
            let arg_string = wire_rat(&arg);
            let word = parse_word(&ctx, &letter_values)?;
            let order = integer_field(request, "order")?;
            if op == "hlog_zero_expand" {
                let terms = hlog_zero_expand(&arg, &word, order)?;
                return Ok(json!({
                    "op": op,
                    "arg": arg_string,
                    "total_str": expansion_total(&terms, &wire_rat(&arg))?,
                    "terms": expansion_value(&terms),
                    "vars": wire_context_variables(&ctx),
                }));
            }

            let result = hlog_series(&arg, &word, variable_index(&ctx, request)?, order)?;
            let (branch, total) = match result.branch {
                HlogSeriesBranch::ZeroLimit => (
                    "zero_limit",
                    expansion_total(&result.terms, &wire_rat(&arg))?,
                ),
                HlogSeriesBranch::Unchanged => {
                    let args = std::iter::once(wire_rat(&arg))
                        .chain(word.letters.iter().map(wire_rat))
                        .collect::<Vec<_>>()
                        .join(",");
                    ("unchanged", format!("Hlog({args})"))
                }
                HlogSeriesBranch::TaylorDeferred => ("taylor_deferred", String::new()),
            };
            Ok(json!({
                "op": op,
                "arg": wire_rat(&arg),
                "branch": branch,
                "total_str": total,
                "terms": expansion_value(&result.terms),
                "vars": wire_context_variables(&ctx),
            }))
        }
        "mpl_series" => {
            let indices = integer_array_field(request, "ns")?;
            let argument_values = string_array_field(request, "zs")?;
            let expressions = argument_values
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let ctx = context_for_variable(request, &expressions)?;
            let arguments = argument_values
                .iter()
                .map(|value| parse_wire_rat(&ctx, value))
                .collect::<Result<Vec<_>>>()?;
            let result = mpl_series(
                &indices,
                &arguments,
                variable_index(&ctx, request)?,
                integer_field(request, "order")?,
            )?;
            let branch = match result.branch {
                MplSeriesBranch::MplSum => "mpl_sum",
                MplSeriesBranch::Unchanged => "unchanged",
                MplSeriesBranch::PolyLogDeferred => "polylog_deferred",
                MplSeriesBranch::LogSingularity => "log_singularity",
                MplSeriesBranch::TaylorDeferred => "taylor_deferred",
            };
            let mut response =
                json!({"op": op, "branch": branch, "vars": wire_context_variables(&ctx)});
            if let Some(scalar) = result.scalar {
                response["scalar"] = Value::String(wire_rat(&scalar));
            }
            Ok(response)
        }
        "pole_degree" | "rat_residue" | "series_expansion" => {
            let expression = string_field(request, "f")?;
            let ctx = context_for_variable(request, &[expression])?;
            let variable = variable_index(&ctx, request)?;
            let function = parse_wire_rat(&ctx, expression)?;
            match op {
                "pole_degree" => {
                    let degree = function.pole_degree(variable)?;
                    let result = if degree == i64::MAX {
                        Value::String("infinity".into())
                    } else {
                        Value::from(degree)
                    };
                    Ok(json!({"op": op, "result": result, "vars": wire_context_variables(&ctx)}))
                }
                "rat_residue" => Ok(result_response(op, &ctx, function.residue(variable)?)),
                "series_expansion" => Ok(result_response(
                    op,
                    &ctx,
                    series_expansion(&function, variable, integer_field(request, "max_order")?)?,
                )),
                _ => unreachable!(),
            }
        }
        _ => unreachable!("operation was checked by series::evaluate"),
    }
}

fn series_table_value(table: &SeriesTable) -> Value {
    Value::Array(
        table
            .iter()
            .map(|row| {
                Value::Array(
                    row.iter()
                        .map(|coefficient| Value::String(wire_rat(coefficient)))
                        .collect(),
                )
            })
            .collect(),
    )
}

fn expansion_value(terms: &ExpansionSeries) -> Value {
    Value::Array(
        terms
            .iter()
            .map(|term| {
                json!({
                    "log_power": term.log_power,
                    "arg_power": term.arg_power,
                    "coef": wire_rat(&term.coef),
                })
            })
            .collect(),
    )
}

fn expansion_total(terms: &ExpansionSeries, arg: &str) -> Result<String> {
    if terms.is_empty() {
        return Ok("0".into());
    }
    let mut summands = Vec::with_capacity(terms.len());
    for term in terms {
        let mut summand = format!("({})", wire_rat(&term.coef));
        if term.log_power > 0 {
            let factorial = (1..=term.log_power).try_fold(1_i64, |accumulator, value| {
                accumulator.checked_mul(value).ok_or_else(|| {
                    Error::InvalidInput(format!(
                        "factorial for log power {} does not fit in i64",
                        term.log_power
                    ))
                })
            })?;
            summand.push_str(&format!("*Log({arg})^{}/{factorial}", term.log_power));
        }
        if term.arg_power > 0 {
            summand.push_str(&format!("*({arg})^{}", term.arg_power));
        }
        summands.push(summand);
    }
    Ok(summands.join(" + "))
}
