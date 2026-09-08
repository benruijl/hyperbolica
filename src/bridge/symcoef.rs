//! Symbolic-coefficient request parsing and arithmetic operations.

use std::fmt::Write;
use std::sync::Arc;

use serde_json::Value;

use super::narrow::payload_strings;
use super::wire::{
    array_field, mzv_context, parse_wire_rat, result_response, string_field,
    unique_diagnostic_variable_index, wire_context_variable, wire_rat,
};
use crate::core::{PolyCtx, SymCoef, SymMonomial, global_period_table, simplify_symcoef};
use crate::error::{Error, Result};

fn legacy_power_key(term: &SymMonomial, deltas: &[(String, i32, usize)]) -> String {
    let mut key = format!("P{}|I{}|L", term.pi_power, term.i_power);
    for (argument, exponent) in &term.log_powers {
        let _ = write!(key, "{argument}:{exponent},");
    }
    key.push_str("|D");
    for (name, exponent, _) in deltas {
        let _ = write!(key, "{name}:{exponent},");
    }
    key.push_str("|Q");
    for (period, exponent) in &term.period_powers {
        let _ = write!(key, "{period}:{exponent},");
    }
    key
}

pub(super) fn evaluate(request: &Value, op: &str) -> Option<Result<Value>> {
    matches!(op, "sym_arith" | "sym_reduce").then(|| evaluate_supported(request, op))
}

fn evaluate_supported(request: &Value, op: &str) -> Result<Value> {
    match op {
        "sym_arith" => {
            let expressions = payload_strings(request, &["a", "b"]);
            let (ctx, _) = mzv_context(request, &expressions)?;
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
            let expressions = payload_strings(request, &["a"]);
            let (ctx, table) = mzv_context(request, &expressions)?;
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
                monomial
                    .delta_powers
                    .insert(unique_diagnostic_variable_index(ctx, variable)?, power);
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

    let mut terms = value
        .terms()
        .iter()
        .map(|term| {
            let mut deltas = term
                .delta_powers
                .iter()
                .map(|(&variable, &power)| {
                    let name = wire_context_variable(value.ctx(), variable)
                        .expect("canonical SymCoef contains a validated delta index");
                    (name, power, variable)
                })
                .collect::<Vec<_>>();
            deltas.sort_unstable();
            let power_key = legacy_power_key(term, &deltas);
            (power_key, deltas, term)
        })
        .collect::<Vec<_>>();
    // Core terms retain typed context-index order. Recreate HyperFLINT's
    // lexical delta-name order only at this legacy string boundary, including
    // the ordering of distinct monomial summands.
    terms.sort_unstable_by(|(left_key, _, left), (right_key, _, right)| {
        left_key
            .cmp(right_key)
            .then_with(|| left.pi_power.cmp(&right.pi_power))
            .then_with(|| left.i_power.cmp(&right.i_power))
            .then_with(|| left.log_powers.cmp(&right.log_powers))
            .then_with(|| left.delta_powers.cmp(&right.delta_powers))
            .then_with(|| left.period_powers.cmp(&right.period_powers))
    });

    terms
        .into_iter()
        .map(|(_, deltas, term)| {
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
            for (name, power, _) in deltas {
                output.push_str(&format!("*delta[{name}]"));
                if power != 1 {
                    output.push_str(&format!("^{power}"));
                }
            }
            for (period, power) in &term.period_powers {
                output.push('*');
                match global_period_table().key_for(*period) {
                    Ok(name) if crate::reduce::mzv_constant_atom(&name).is_some() => {
                        output.push_str(&name)
                    }
                    _ => output.push_str(&format!("Period[{period}]")),
                }
                if *power != 1 {
                    output.push_str(&format!("^{power}"));
                }
            }
            output
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use symbolica::prelude::Symbol;

    use super::*;

    #[test]
    fn period_basis_keys_emit_in_legacy_spelling() {
        let ctx = PolyCtx::new(["bridge_period_basis_x"]).unwrap();
        let id = global_period_table().id_for("mzv_2").unwrap();
        let value = SymCoef::period_factor(ctx, id);
        assert_eq!(symcoef_string(&value), "1*mzv_2");
    }

    #[test]
    fn opaque_period_keys_are_not_interpreted_as_expressions() {
        let ctx = PolyCtx::new(["bridge_opaque_period_x"]).unwrap();
        let id = global_period_table().id_for("a+b").unwrap();
        let mut monomial = SymMonomial::new(crate::core::Rat::one(ctx.clone()));
        monomial.period_powers.insert(id, 2);
        let value = SymCoef::from_monomials(ctx, vec![monomial]);
        assert_eq!(symcoef_string(&value), format!("1*Period[{id}]^2"));
    }

    #[test]
    fn legacy_delta_input_rejects_an_ambiguous_diagnostic_name() {
        let left = Symbol::parse("x", "bridge_delta_left").unwrap();
        let right = Symbol::parse("x", "bridge_delta_right").unwrap();
        let ctx = PolyCtx::from_indeterminates([left.to_atom(), right.to_atom()]).unwrap();
        let request = json!({"a": [{"deltas": [["x", 1]]}]});

        assert!(matches!(
            parse_symcoef(&ctx, &request, "a"),
            Err(Error::InvalidInput(message)) if message.contains("ambiguous")
        ));
    }

    #[test]
    fn legacy_delta_input_resolves_once_to_a_context_index() {
        let symbol = Symbol::parse("x", "bridge_delta_unique").unwrap();
        let ctx = PolyCtx::from_indeterminates([symbol.to_atom()]).unwrap();
        let request = json!({"a": [{"deltas": [["x", 1]]}]});
        let coefficient = parse_symcoef(&ctx, &request, "a").unwrap();

        assert_eq!(coefficient.terms()[0].delta_powers.get(&0), Some(&1));
        assert!(symcoef_string(&coefficient).contains("delta[x]"));
    }

    #[test]
    fn registered_delta_names_resolve_and_emit_in_legacy_spelling() {
        let mzv = crate::symbols::mzv_atom(&[2]);
        let wm = crate::symbols::algebraic_atoms(1).minus;
        let ctx = PolyCtx::from_indeterminates([mzv, wm]).unwrap();
        let request = json!({
            "a": [{"deltas": [["mzv_2", 1], ["Wm_1", 1]]}]
        });
        let coefficient = parse_symcoef(&ctx, &request, "a").unwrap();

        assert_eq!(coefficient.terms()[0].delta_powers.get(&0), Some(&1));
        assert_eq!(coefficient.terms()[0].delta_powers.get(&1), Some(&1));
        assert_eq!(symcoef_string(&coefficient), "1*delta[Wm_1]*delta[mzv_2]");
    }

    #[test]
    fn delta_factors_and_monomials_emit_in_name_order_not_context_order() {
        let ctx = PolyCtx::new(["z", "x"]).unwrap();

        let mut both = SymMonomial::new(crate::core::Rat::one(ctx.clone()));
        both.delta_powers.insert(0, 1);
        both.delta_powers.insert(1, 1);
        let both = SymCoef::from_monomials(ctx.clone(), vec![both]);
        let both_output = symcoef_string(&both);
        assert_eq!(both_output.as_bytes(), b"1*delta[x]*delta[z]");

        let mut z = SymMonomial::new(crate::core::Rat::one(ctx.clone()));
        z.delta_powers.insert(0, 1);
        let mut x = SymMonomial::new(crate::core::Rat::one(ctx.clone()));
        x.delta_powers.insert(1, 1);
        let sum = SymCoef::from_monomials(ctx, vec![z, x]);
        assert_eq!(symcoef_string(&sum), "1*delta[x] + 1*delta[z]");
    }

    #[test]
    fn monomial_terms_preserve_legacy_lexical_power_key_order() {
        let ctx = PolyCtx::new(["x"]).unwrap();

        let mut pi_two = SymMonomial::new(crate::core::Rat::one(ctx.clone()));
        pi_two.pi_power = 2;
        let mut pi_ten = SymMonomial::new(crate::core::Rat::one(ctx.clone()));
        pi_ten.pi_power = 10;
        let pi = SymCoef::from_monomials(ctx.clone(), vec![pi_two, pi_ten]);
        assert_eq!(symcoef_string(&pi), "1*Pi^10 + 1*Pi^2");

        let mut log_two = SymMonomial::new(crate::core::Rat::one(ctx.clone()));
        log_two.log_powers.insert(2, 1);
        let mut log_ten = SymMonomial::new(crate::core::Rat::one(ctx.clone()));
        log_ten.log_powers.insert(10, 1);
        let logs = SymCoef::from_monomials(ctx, vec![log_two, log_ten]);
        assert_eq!(symcoef_string(&logs), "1*Log[10] + 1*Log[2]");
    }
}
