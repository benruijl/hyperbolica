//! Linear-reducibility search, scan, and factor-table protocol operations.

use std::sync::Arc;
use std::time::Instant;

use serde_json::{Value, json};

use super::SCHEMA_VERSION;
use super::wire::{
    array_field, optional_bool, optional_usize, parse_wire_poly, string_array_field,
    wire_context_variables, wire_poly,
};
use crate::core::{Poly, PolyCtx};
use crate::error::{Error, Result};
use crate::integrator::factor_table::{
    FactorTableLimits, FactoredObject, factor_table as build_factor_table,
};
use crate::integrator::lr_scan::{KeepRule, ScanExponent, ScanOptions, find_lr_orders_scan};
use crate::integrator::lr_search::{
    LrSearchOptions, SingCollector, find_lr_orders, find_lr_orders_collect,
};
use crate::integrator::lr_verify::{OrderVerifyResult, verify_order_is_lr};

type LrInput = (Arc<PolyCtx>, Vec<String>, Vec<Vec<Poly>>, Vec<usize>);

pub(super) fn evaluate(request: &Value, op: &str) -> Option<Result<Value>> {
    matches!(
        op,
        "find_lr_orders" | "find_lr_orders_scan" | "factor_table"
    )
    .then(|| evaluate_supported(request, op))
}

fn evaluate_supported(request: &Value, op: &str) -> Result<Value> {
    match op {
        "find_lr_orders" => {
            let (ctx, xvars, groups, xvar_indices) = lr_input(request)?;
            let allow_algebraic_letters = optional_bool(request, "algebraic_letters", false)?;
            let carry_discharge = optional_bool(request, "carry_discharge", false)?;
            let score_prune_factor = match request.get("score_prune_factor") {
                None => f64::INFINITY,
                Some(value) => value.as_f64().ok_or_else(|| {
                    Error::InvalidInput("`score_prune_factor` must be a number".into())
                })?,
            };
            let options = LrSearchOptions {
                allow_algebraic_letters,
                carry_discharge,
                score_prune_factor,
                euler_filter: optional_bool(request, "euler_filter", false)?
                    || legacy_euler_environment(),
            };
            let emit_sings = optional_bool(request, "emit_sings", false)?;
            let verify_order = request
                .get("verify_order")
                .map(|_| string_array_field(request, "verify_order"))
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
                response["kin_sqrts"] =
                    Value::from(result.as_ref().map_or(0, |result| result.kin_sqrts));
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
        "find_lr_orders_scan" => {
            let (ctx, xvars, groups, xvar_indices) = lr_input(request)?;
            let exponents = array_field(request, "exps")?
                .iter()
                .map(|group| {
                    group
                        .as_array()
                        .ok_or_else(|| Error::InvalidInput("`exps` groups must be arrays".into()))?
                        .iter()
                        .map(|pair| {
                            let pair = pair.as_array().ok_or_else(|| {
                                Error::InvalidInput("`exps` entries must be [a,b]".into())
                            })?;
                            if pair.len() != 2 {
                                return Err(Error::InvalidInput(
                                    "`exps` entries must contain exactly two integers".into(),
                                ));
                            }
                            Ok(ScanExponent {
                                a: pair[0].as_i64().ok_or_else(|| {
                                    Error::InvalidInput("exponent a must be an integer".into())
                                })?,
                                b: pair[1].as_i64().ok_or_else(|| {
                                    Error::InvalidInput("exponent b must be an integer".into())
                                })?,
                            })
                        })
                        .collect()
                })
                .collect::<Result<Vec<Vec<_>>>>()?;
            let keep_rule = match request
                .get("keep_rule")
                .and_then(Value::as_str)
                .unwrap_or("Strict")
            {
                "Strict" => KeepRule::Strict,
                "FindRoots" => KeepRule::FindRoots,
                other => {
                    return Err(Error::InvalidInput(format!("unknown keep_rule `{other}`")));
                }
            };
            let options = ScanOptions {
                keep_rule,
                euler_filter: optional_bool(request, "euler_filter", false)?
                    || legacy_euler_environment(),
                max_orders: optional_usize(request, "max_orders", 8192)?,
            };
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
        "factor_table" => {
            let (ctx, xvars, groups, _) = lr_input(request)?;
            let order = string_array_field(request, "order")?;
            let mut sorted_order = order.clone();
            let mut sorted_variables = xvars.clone();
            sorted_order.sort_unstable();
            sorted_variables.sort_unstable();
            if sorted_order != sorted_variables {
                return Err(Error::InvalidInput(
                    "`order` must be a permutation of `xvars`".into(),
                ));
            }
            let order_indices = order
                .iter()
                .map(|name| {
                    crate::symbols::legacy::atom_from_name(name)
                        .ok()
                        .and_then(|atom| ctx.index_of_indeterminate(atom.as_view()))
                        .ok_or_else(|| Error::UnknownVariable(name.clone()))
                })
                .collect::<Result<Vec<_>>>()?;
            let limits = FactorTableLimits {
                max_pairs: optional_usize(request, "max_pairs", 2_000_000)?,
                max_singletons: optional_usize(request, "max_singletons", 200_000)?,
                max_response_mb: optional_usize(request, "max_response_mb", 512)?,
            };
            let table = build_factor_table(
                &groups,
                &order_indices,
                optional_bool(request, "algebraic_letters", false)?,
                limits,
            )?;
            // Formatting is deliberately confined to this transport
            // boundary. The factor-table interner and all LR caches use
            // Symbolica's structural polynomial equality and hashing.
            let wire_polys = table.intern_polys.iter().map(wire_poly).collect::<Vec<_>>();
            let mut wire_pairs = table
                .pairs
                .iter()
                .map(|pair| {
                    // HyperFLINT historically orients a pair by the lexical
                    // wire spelling. Internally we use ID order and avoid
                    // formatting; restore the old orientation (and the sign
                    // of g' f - f' g) only while serializing.
                    let swapped = wire_polys[pair.g_id] < wire_polys[pair.f_id];
                    let (f_id, g_id) = if swapped {
                        (pair.g_id, pair.f_id)
                    } else {
                        (pair.f_id, pair.g_id)
                    };
                    let mut value = factored_object_value_signed(&pair.difference, swapped);
                    let object = value.as_object_mut().expect("factored object is a map");
                    object.insert(
                        "var".into(),
                        Value::String(wire_context_variables(&ctx)[pair.var_idx].clone()),
                    );
                    object.insert("f".into(), Value::from(f_id));
                    object.insert("g".into(), Value::from(g_id));
                    (pair.var_idx, f_id, g_id, value)
                })
                .collect::<Vec<_>>();
            wire_pairs.sort_by_key(|(variable, f_id, g_id, _)| (*variable, *f_id, *g_id));
            let wire_pairs = wire_pairs
                .into_iter()
                .map(|(_, _, _, value)| value)
                .collect::<Vec<_>>();
            let response = json!({
                "op": op,
                "schema_version": SCHEMA_VERSION,
                "hf_version": env!("CARGO_PKG_VERSION"),
                "order": order,
                "polys": wire_polys,
                "stages": table.stages.iter().map(|stage| json!({
                    "var": wire_context_variables(&ctx)[stage.var_idx],
                    "admissible": stage.admissible,
                    "pool": stage.pool,
                    "n_pairs": stage.pair_count,
                    "n_singletons": stage.singleton_count,
                    "n_inadmissible": stage.inadmissible_count,
                    "t_build_s": stage.build_seconds,
                })).collect::<Vec<_>>(),
                "pairs": wire_pairs,
                "singletons": table.singletons.iter().map(|entry| json!({
                    "var": wire_context_variables(&ctx)[entry.var_idx],
                    "id": entry.id,
                    "deg": entry.degree,
                    "coeffs": entry.coefficients.iter().map(|coefficient| {
                        let mut value = factored_object_value(&coefficient.object);
                        value.as_object_mut().expect("factored object is a map")
                            .insert("power".into(), Value::from(coefficient.power));
                        value
                    }).collect::<Vec<_>>(),
                    "disc": entry.discriminant.as_ref().map(factored_object_value),
                })).collect::<Vec<_>>(),
                "stats": {
                    "pairs_total": table.stats.pairs_total,
                    "singletons_total": table.stats.singletons_total,
                    "oop": table.stats.oop,
                    "pair_fallbacks": table.stats.pair_fallbacks,
                    "trial_s": table.stats.trial_seconds,
                    "fallback_s": table.stats.fallback_seconds,
                },
            });
            let response_size = serde_json::to_vec(&response)?.len();
            let maximum_size = limits.max_response_mb.saturating_mul(1024 * 1024);
            if response_size > maximum_size {
                return Err(Error::InvalidInput(format!(
                    "max_response_mb exceeded ({response_size} bytes, cap {maximum_size} bytes)"
                )));
            }
            Ok(response)
        }
        _ => unreachable!("operation was checked by lr::evaluate"),
    }
}

fn legacy_euler_environment() -> bool {
    std::env::var("HF_EULER_FILTER").is_ok_and(|value| !value.is_empty() && value != "0")
}

fn append_verify_result(response: &mut Value, result: &OrderVerifyResult) {
    response["order_is_lr"] = Value::Bool(result.is_lr);
    response["verify_malformed"] = Value::Bool(result.malformed);
    response["verify_blocking_step"] = Value::from(result.blocking_step);
    response["verify_blocking_degree"] = Value::from(result.blocking_degree);
    response["verify_forbidden_dep"] = Value::Bool(result.forbidden_dep);
    response["verify_blocking_letter"] = Value::String(
        result
            .blocking_letter
            .as_ref()
            .map(wire_poly)
            .unwrap_or_default(),
    );
}

fn nested_string_arrays(request: &Value, name: &str) -> Result<Vec<Vec<String>>> {
    array_field(request, name)?
        .iter()
        .map(|group| {
            group
                .as_array()
                .ok_or_else(|| Error::InvalidInput(format!("`{name}` entries must be arrays")))?
                .iter()
                .map(|value| {
                    value.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                        Error::InvalidInput(format!("`{name}` polynomial entries must be strings"))
                    })
                })
                .collect()
        })
        .collect()
}

fn lr_input(request: &Value) -> Result<LrInput> {
    let xvars = string_array_field(request, "xvars")?;
    if xvars.is_empty() {
        return Err(Error::InvalidInput("`xvars` must not be empty".into()));
    }
    let coefficient_vars = request
        .get("coeff_vars")
        .map(|_| string_array_field(request, "coeff_vars"))
        .transpose()?
        .unwrap_or_default();
    let mut all_vars = xvars.clone();
    all_vars.extend(coefficient_vars);
    let all_vars = all_vars
        .iter()
        .map(|name| crate::symbols::legacy::atom_from_name(name))
        .collect::<Result<Vec<_>>>()?;
    let ctx = PolyCtx::from_indeterminates(all_vars)?;
    let groups = if request.get("groups").is_some() {
        nested_string_arrays(request, "groups")?
    } else {
        vec![string_array_field(request, "polys")?]
    };
    if groups.is_empty() || groups.iter().any(Vec::is_empty) {
        return Err(Error::InvalidInput(
            "polynomial groups must not be empty".into(),
        ));
    }
    let groups = groups
        .into_iter()
        .map(|group| {
            group
                .into_iter()
                .map(|expression| parse_wire_poly(&ctx, &expression))
                .collect()
        })
        .collect::<Result<Vec<_>>>()?;
    let indices = xvars
        .iter()
        .map(|name| {
            crate::symbols::legacy::atom_from_name(name)
                .ok()
                .and_then(|atom| ctx.index_of_indeterminate(atom.as_view()))
                .ok_or_else(|| Error::UnknownVariable(name.clone()))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((ctx, xvars, groups, indices))
}

fn factored_object_value(object: &FactoredObject) -> Value {
    factored_object_value_signed(object, false)
}

fn factored_object_value_signed(object: &FactoredObject, negate: bool) -> Value {
    let constant = if negate {
        (-object.constant.clone()).to_string()
    } else {
        object.constant.to_string()
    };
    json!({
        "c": constant,
        "factors": object.factors.iter().map(|(id, exponent)| json!([id, exponent])).collect::<Vec<_>>(),
        "oop": object.oop,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn json_lr_request_accepts_explicit_euler_flag() {
        let request = json!({
            "op": "find_lr_orders",
            "xvars": ["x"],
            "groups": [["x"]],
            "euler_filter": true,
        });
        let response = evaluate_supported(&request, "find_lr_orders").unwrap();
        assert_eq!(response["op"], "find_lr_orders");
        assert!(response.get("nolr").is_some());
    }

    #[test]
    fn verify_order_reports_malformed_permutations_in_band() {
        let request = json!({
            "op": "find_lr_orders",
            "xvars": ["x", "y"],
            "groups": [["x+y"]],
            "verify_order": ["x", "x"],
        });
        let response = evaluate_supported(&request, "find_lr_orders").unwrap();
        assert_eq!(response["order_is_lr"], false);
        assert_eq!(response["verify_malformed"], true);
        assert_eq!(response["verify_blocking_step"], -1);
        assert_eq!(response["verify_blocking_degree"], 0);
        assert_eq!(response["verify_forbidden_dep"], false);
        assert_eq!(response["verify_blocking_letter"], "");
        assert_eq!(response["best_order"], json!([]));
        assert_eq!(response["score"], Value::Null);
        assert_eq!(response["nolr"], false);
    }

    #[test]
    fn verify_order_keeps_carry_inert_and_emits_the_compatibility_envelope() {
        let request = json!({
            "op": "find_lr_orders",
            "xvars": ["x", "y"],
            "groups": [["x^2*y+x+1", "x", "y"]],
            "verify_order": ["x", "y"],
            "algebraic_letters": true,
            "carry_discharge": true,
            "emit_sings": true,
        });
        let response = evaluate_supported(&request, "find_lr_orders").unwrap();
        assert_eq!(response["order_is_lr"], false);
        assert_eq!(response["verify_malformed"], false);
        assert_eq!(response["verify_blocking_step"], 0);
        assert_eq!(response["verify_blocking_degree"], 2);
        assert_eq!(response["verify_forbidden_dep"], true);
        assert_eq!(response["verify_blocking_letter"], "x^2*y + x + 1");
        assert_eq!(response["strategy"], "LR_OptOrdered");
        assert_eq!(response["root_polys"], json!([]));
        assert_eq!(response["carried_sqrts"], 0);
        assert_eq!(response["kin_sqrts"], 0);
        assert_eq!(response["terminal_quads"], 0);
        assert_eq!(response["carried_polys"], json!([]));
        assert_eq!(response["sings"], json!([]));
        assert_eq!(response["sings_total"], 0);
    }

    #[test]
    fn verify_order_uses_the_requested_algebraic_degree_cap() {
        let strict = json!({
            "op": "find_lr_orders",
            "xvars": ["x", "y"],
            "groups": [["1+x+x^2", "x", "y"]],
            "verify_order": ["x", "y"],
        });
        let strict_response = evaluate_supported(&strict, "find_lr_orders").unwrap();
        assert_eq!(strict_response["order_is_lr"], false);
        assert_eq!(strict_response["strategy"], "LR_NoOpt");

        let mut algebraic = strict;
        algebraic["algebraic_letters"] = Value::Bool(true);
        algebraic["carry_discharge"] = Value::Bool(true);
        let algebraic_response = evaluate_supported(&algebraic, "find_lr_orders").unwrap();
        assert_eq!(algebraic_response["order_is_lr"], true);
        assert_eq!(algebraic_response["strategy"], "LR_OptOrdered");
    }

    #[test]
    fn factor_table_formats_and_lexically_orients_only_at_the_wire_boundary() {
        // Encounter order is deliberately opposite to lexical order. The
        // structural table uses ID order internally, while the JSON adapter
        // must preserve HyperFLINT's historical f/g orientation and sign.
        let request = json!({
            "op": "factor_table",
            "xvars": ["x"],
            "groups": [["x+1", "x"]],
            "order": ["x"],
        });
        let response = evaluate_supported(&request, "factor_table").unwrap();

        assert_eq!(response["polys"], json!(["x + 1", "x"]));
        assert_eq!(response["pairs"].as_array().unwrap().len(), 1);
        assert_eq!(response["pairs"][0]["f"], 1);
        assert_eq!(response["pairs"][0]["g"], 0);
        assert_eq!(response["pairs"][0]["c"], "-1");
        assert_eq!(response["pairs"][0]["factors"], json!([]));
    }

    #[test]
    fn singularity_polynomials_are_structural_until_json_serialization() {
        let request = json!({
            "op": "find_lr_orders",
            "xvars": ["x"],
            "coeff_vars": ["s"],
            "groups": [["x+s"]],
            "emit_sings": true,
        });
        let response = evaluate_supported(&request, "find_lr_orders").unwrap();

        assert_eq!(response["sings"], json!(["s"]));
        assert_eq!(response["sings_total"], 1);
    }

    #[test]
    fn registered_singularity_uses_its_legacy_wire_spelling() {
        let request = json!({
            "op": "find_lr_orders",
            "xvars": ["x"],
            "coeff_vars": ["Wm_1"],
            "groups": [["x+Wm_1"]],
            "emit_sings": true,
        });
        let response = evaluate_supported(&request, "find_lr_orders").unwrap();

        assert_eq!(response["sings"], json!(["Wm_1"]));
        assert_eq!(response["sings_total"], 1);
    }

    #[test]
    fn registered_blocker_stays_typed_until_legacy_wire_serialization() {
        let request = json!({
            "op": "find_lr_orders",
            "xvars": ["x"],
            "coeff_vars": ["mzv_2"],
            "groups": [["x^2+mzv_2"]],
            "verify_order": ["x"],
        });
        let response = evaluate_supported(&request, "find_lr_orders").unwrap();
        let blocker = response["verify_blocking_letter"].as_str().unwrap();

        assert!(blocker.contains("mzv_2"));
        assert!(blocker.contains("x^2"));
        assert!(!blocker.contains("hyperbolica::MZV"));
    }

    #[test]
    fn registered_factor_table_atoms_drive_legacy_lexical_pair_orientation() {
        // Native `MZV` sorts before native `Wp`, while their historical wire
        // spellings have the opposite byte order (`Wp_1` < `mzv_2`). Pair
        // orientation must therefore use the already-exported wire strings.
        let request = json!({
            "op": "factor_table",
            "xvars": ["x"],
            "coeff_vars": ["mzv_2", "Wp_1"],
            "groups": [["x+mzv_2", "x+Wp_1"]],
            "order": ["x"],
        });
        let response = evaluate_supported(&request, "factor_table").unwrap();
        let polynomials = response["polys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(
            polynomials
                .iter()
                .all(|polynomial| !polynomial.contains("hyperbolica::"))
        );

        let mzv_id = polynomials
            .iter()
            .position(|polynomial| polynomial.contains("mzv_2") && polynomial.contains('x'))
            .unwrap();
        let wp_id = polynomials
            .iter()
            .position(|polynomial| polynomial.contains("Wp_1") && polynomial.contains('x'))
            .unwrap();
        assert!(polynomials[wp_id] < polynomials[mzv_id]);

        let pair = response["pairs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|pair| {
                let f = pair["f"].as_u64().unwrap() as usize;
                let g = pair["g"].as_u64().unwrap() as usize;
                (f == mzv_id && g == wp_id) || (f == wp_id && g == mzv_id)
            })
            .unwrap();
        assert_eq!(pair["f"], wp_id);
        assert_eq!(pair["g"], mzv_id);
    }
}
