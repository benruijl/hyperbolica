use std::time::Instant;

use serde_json::{Value, json};

use super::super::SCHEMA_VERSION;
use super::super::wire::{optional_bool, wire_context_variables, wire_poly};
use super::input::{legacy_euler_environment, lr_input, scan_exponents, scan_options};
use super::verify::append_verify_result;
use crate::error::{Error, Result};
use crate::integrator::lr_scan::find_lr_orders_scan;
use crate::integrator::lr_search::{
    LrSearchOptions, SingCollector, find_lr_orders, find_lr_orders_collect,
};
use crate::integrator::lr_verify::verify_order_is_lr;

pub(super) fn evaluate_find_orders(request: &Value, op: &str) -> Result<Value> {
    let (ctx, xvars, groups, xvar_indices) = lr_input(request)?;
    let allow_algebraic_letters = optional_bool(request, "algebraic_letters", false)?;
    let carry_discharge = optional_bool(request, "carry_discharge", false)?;
    let score_prune_factor = match request.get("score_prune_factor") {
        None => f64::INFINITY,
        Some(value) => value
            .as_f64()
            .ok_or_else(|| Error::InvalidInput("`score_prune_factor` must be a number".into()))?,
    };
    let options = LrSearchOptions {
        allow_algebraic_letters,
        carry_discharge,
        score_prune_factor,
        // Upstream find_lr_orders is controlled only by HF_EULER_FILTER;
        // the similarly named JSON option belongs to the scan operation.
        euler_filter: legacy_euler_environment(),
    };
    let emit_sings = optional_bool(request, "emit_sings", false)?;
    let verify_order = request
        .get("verify_order")
        .map(|_| super::super::wire::string_array_field(request, "verify_order"))
        .transpose()?;
    let started = Instant::now();
    let mut collector = SingCollector::default();
    let (result, verify_result) = if let Some(order) = &verify_order {
        let order_indices = order
            .iter()
            .map(|name| {
                xvars
                    .iter()
                    .position(|candidate| candidate == name)
                    .map(|position| xvar_indices[position])
                    .unwrap_or(usize::MAX)
            })
            .collect::<Vec<_>>();
        (
            None,
            Some(verify_order_is_lr(
                &groups,
                &xvar_indices,
                &order_indices,
                allow_algebraic_letters,
            )?),
        )
    } else if emit_sings {
        (
            Some(find_lr_orders_collect(
                &groups,
                &xvar_indices,
                options,
                Some(&mut collector),
            )?),
            None,
        )
    } else {
        (Some(find_lr_orders(&groups, &xvar_indices, options)?), None)
    };
    let best_order = result
        .as_ref()
        .map(|result| {
            result
                .order
                .iter()
                .map(|&index| wire_context_variables(&ctx)[index].clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let method = request
        .get("method_lr_hint")
        .and_then(Value::as_str)
        .unwrap_or("Lungo");
    let nolr = result.as_ref().is_some_and(|result| result.is_nolr());
    let strategy = if nolr {
        if method == "Espresso" {
            "Fubini_Espresso"
        } else {
            "Fubini_Lungo"
        }
    } else if allow_algebraic_letters {
        "LR_OptOrdered"
    } else {
        "LR_NoOpt"
    };
    let mut response = json!({
        "op": op,
        "schema_version": SCHEMA_VERSION,
        "hf_version": env!("CARGO_PKG_VERSION"),
        "best_order": best_order,
        "score": result.as_ref().and_then(|result| {
            (!result.is_nolr() && result.score.is_finite())
                .then(|| Value::from(result.score))
        }).unwrap_or(Value::Null),
        "nolr": nolr,
        "strategy": strategy,
        "timing_compute_s": started.elapsed().as_secs_f64(),
        "nXVars": xvars.len(),
        "nGroups": groups.len(),
        "nPolys": groups.iter().map(Vec::len).collect::<Vec<_>>(),
    });
    if allow_algebraic_letters {
        response["root_polys"] = Value::Array(
            result
                .as_ref()
                .map(|result| {
                    result
                        .root_polys
                        .iter()
                        .map(|polynomial| Value::String(wire_poly(polynomial)))
                        .collect()
                })
                .unwrap_or_default(),
        );
    }
    if allow_algebraic_letters && carry_discharge {
        response["carried_sqrts"] =
            Value::from(result.as_ref().map_or(0, |result| result.carried_sqrts));
        response["kin_sqrts"] = Value::from(result.as_ref().map_or(0, |result| result.kin_sqrts));
        response["terminal_quads"] =
            Value::from(result.as_ref().map_or(0, |result| result.terminal_quads));
        response["carried_polys"] = Value::Array(
            result
                .as_ref()
                .map(|result| {
                    result
                        .obligation_polys
                        .iter()
                        .map(|polynomial| Value::String(wire_poly(polynomial)))
                        .collect()
                })
                .unwrap_or_default(),
        );
    }
    if emit_sings {
        response["sings"] = Value::Array(
            collector
                .singularities()
                .iter()
                .map(|polynomial| Value::String(wire_poly(polynomial)))
                .collect(),
        );
        response["sings_total"] = Value::from(collector.singularities().len());
    }
    if let Some(verify) = verify_result {
        append_verify_result(&mut response, &verify);
    }
    Ok(response)
}

pub(super) fn evaluate_scan(request: &Value, op: &str) -> Result<Value> {
    let (ctx, xvars, groups, xvar_indices) = lr_input(request)?;
    let exponents = scan_exponents(request)?;
    let options = scan_options(request)?;
    let started = Instant::now();
    let result = find_lr_orders_scan(&groups, &xvar_indices, &exponents, options)?;
    Ok(json!({
        "op": op,
        "schema_version": SCHEMA_VERSION,
        "hf_version": env!("CARGO_PKG_VERSION"),
        "projective": result.projective,
        "truncated": result.truncated,
        "orders": result.orders.iter().map(|order| json!({
            "order": order.order.iter().map(|&index| wire_context_variables(&ctx)[index].clone()).collect::<Vec<_>>(),
            "gauge": wire_context_variables(&ctx)[order.gauge],
            "score": order.score,
            "carried_sqrts": order.carried_sqrts,
            "kin_sqrts": order.kin_sqrts,
            "terminal_quads": order.terminal_quads,
        })).collect::<Vec<_>>(),
        "timing_compute_s": started.elapsed().as_secs_f64(),
        "nXVars": xvars.len(),
        "nGroups": groups.len(),
    }))
}
