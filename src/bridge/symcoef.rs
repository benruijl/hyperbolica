//! Symbolic-coefficient request parsing and arithmetic operations.

use std::sync::Arc;

use serde_json::Value;

use super::wire::{
    array_field, mzv_context, parse_wire_rat, result_response, string_field, wire_rat,
};
use crate::core::{PolyCtx, SymCoef, SymMonomial, simplify_symcoef};
use crate::error::{Error, Result};

pub(super) fn evaluate(request: &Value, op: &str) -> Option<Result<Value>> {
    matches!(op, "sym_arith" | "sym_reduce").then(|| evaluate_supported(request, op))
}

fn evaluate_supported(request: &Value, op: &str) -> Result<Value> {
    match op {
        "sym_arith" => {
            let (ctx, _) = mzv_context(request, &[])?;
            let left = parse_symcoef(&ctx, request, "a")?;
            let right = parse_symcoef(&ctx, request, "b")?;
            let result = match string_field(request, "mode")? {
                "add" => left.try_add(&right)?,
                "sub" => left.try_sub(&right)?,
                "mul" => left.try_mul(&right)?,
                mode => {
                    return Err(Error::InvalidInput(format!(
                        "unknown sym_arith mode `{mode}`"
                    )));
                }
            };
            Ok(result_response(op, &ctx, symcoef_string(&result)))
        }
        "sym_reduce" => {
            let (ctx, table) = mzv_context(request, &[])?;
            let input = parse_symcoef(&ctx, request, "a")?;
            let result = simplify_symcoef(&input, &table)?;
            Ok(result_response(op, &ctx, symcoef_string(&result)))
        }
        _ => unreachable!("operation was checked by symcoef::evaluate"),
    }
}

fn parse_symcoef(ctx: &Arc<PolyCtx>, request: &Value, name: &str) -> Result<SymCoef> {
    let mut monomials = Vec::new();
    for value in array_field(request, name)? {
        let prefactor = value
            .get("prefactor")
            .and_then(Value::as_str)
            .unwrap_or("1");
        let mut monomial = SymMonomial::new(parse_wire_rat(ctx, prefactor)?);
        monomial.pi_power = value
            .get("pi")
            .map(|power| {
                power
                    .as_i64()
                    .and_then(|power| i32::try_from(power).ok())
                    .filter(|power| *power >= 0)
                    .ok_or_else(|| {
                        Error::InvalidInput("symbolic Pi power must be a non-negative i32".into())
                    })
            })
            .transpose()?
            .unwrap_or(0);
        monomial.i_power = value
            .get("i")
            .map(|power| {
                power
                    .as_i64()
                    .and_then(|power| i32::try_from(power).ok())
                    .filter(|power| *power >= 0)
                    .ok_or_else(|| {
                        Error::InvalidInput("symbolic I power must be a non-negative i32".into())
                    })
            })
            .transpose()?
            .unwrap_or(0);

        if let Some(logs) = value.get("logs") {
            for pair in logs
                .as_array()
                .ok_or_else(|| Error::InvalidInput("`logs` must be an array".into()))?
            {
                let pair = pair.as_array().ok_or_else(|| {
                    Error::InvalidInput("log powers must be [argument,power]".into())
                })?;
                if pair.len() != 2 {
                    return Err(Error::InvalidInput(
                        "log powers must contain two integers".into(),
                    ));
                }
                let argument = pair[0].as_i64().filter(|value| *value > 0).ok_or_else(|| {
                    Error::InvalidInput("log argument must be a positive integer".into())
                })?;
                let power = pair[1]
                    .as_i64()
                    .and_then(|value| i32::try_from(value).ok())
                    .filter(|value| *value >= 0)
                    .ok_or_else(|| {
                        Error::InvalidInput("log power must be a non-negative i32".into())
                    })?;
                monomial.log_powers.insert(argument, power);
            }
        }
        if let Some(deltas) = value.get("deltas") {
            for pair in deltas
                .as_array()
                .ok_or_else(|| Error::InvalidInput("`deltas` must be an array".into()))?
            {
                let pair = pair.as_array().ok_or_else(|| {
                    Error::InvalidInput("delta powers must be [name,power]".into())
                })?;
                if pair.len() != 2 {
                    return Err(Error::InvalidInput(
                        "delta powers must contain a name and integer".into(),
                    ));
                }
                let variable = pair[0]
                    .as_str()
                    .ok_or_else(|| Error::InvalidInput("delta name must be a string".into()))?;
                let power = pair[1]
                    .as_i64()
                    .and_then(|value| i32::try_from(value).ok())
                    .filter(|value| *value >= 0)
                    .ok_or_else(|| {
                        Error::InvalidInput("delta power must be a non-negative i32".into())
                    })?;
                monomial.delta_powers.insert(variable.into(), power);
            }
        }
        monomials.push(monomial);
    }
    SymCoef::try_from_monomials(ctx.clone(), monomials)
}

pub(super) fn symcoef_string(value: &SymCoef) -> String {
    if value.is_zero() {
        return "0".into();
    }
    value
        .terms()
        .iter()
        .map(|term| {
            let prefactor = wire_rat(&term.prefactor);
            let mut output = if prefactor.contains(['+', '-', '/', '*']) {
                format!("({prefactor})")
            } else {
                prefactor
            };
            if term.pi_power != 0 {
                output.push_str("*Pi");
                if term.pi_power != 1 {
                    output.push_str(&format!("^{}", term.pi_power));
                }
            }
            if term.i_power != 0 {
                output.push_str("*I");
                if term.i_power != 1 {
                    output.push_str(&format!("^{}", term.i_power));
                }
            }
            for (argument, power) in &term.log_powers {
                output.push_str(&format!("*Log[{argument}]"));
                if *power != 1 {
                    output.push_str(&format!("^{power}"));
                }
            }
            for (variable, power) in &term.delta_powers {
                output.push_str(&format!("*delta[{variable}]"));
                if *power != 1 {
                    output.push_str(&format!("^{power}"));
                }
            }
            for (period, power) in &term.period_powers {
                output.push_str(&format!("*Period[{period}]"));
                if *power != 1 {
                    output.push_str(&format!("^{power}"));
                }
            }
            output
        })
        .collect::<Vec<_>>()
        .join(" + ")
}
