use serde_json::{Value, json};

use super::super::SCHEMA_VERSION;
use super::super::wire::{
    optional_bool, optional_usize, string_array_field, wire_context_variables, wire_poly,
};
use super::input::lr_input;
use crate::error::{Error, Result};
use crate::integrator::factor_table::{
    FactorTableLimits, FactoredObject, factor_table as build_factor_table,
};

pub(super) fn evaluate(request: &Value, op: &str) -> Result<Value> {
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
    // Formatting is deliberately confined to this transport boundary. The
    // factor-table interner and all LR caches use Symbolica's structural
    // polynomial equality and hashing.
    let wire_polys = table.intern_polys.iter().map(wire_poly).collect::<Vec<_>>();
    let mut wire_pairs = table
        .pairs
        .iter()
        .map(|pair| {
            // HyperFLINT historically orients a pair by the lexical wire
            // spelling. Internally we use ID order and avoid formatting;
            // restore the old orientation (and the sign of g' f - f' g) only
            // while serializing.
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
        "singletons": table.singletons.iter().map(|entry| {
            let mut value = json!({
                "var": wire_context_variables(&ctx)[entry.var_idx],
                "id": entry.id,
                "deg": entry.degree,
                "coeffs": entry.coefficients.iter().map(|coefficient| {
                    let mut value = factored_object_value(&coefficient.object);
                    value.as_object_mut().expect("factored object is a map")
                        .insert("power".into(), Value::from(coefficient.power));
                    value
                }).collect::<Vec<_>>(),
            });
            if let Some(discriminant) = &entry.discriminant {
                value.as_object_mut().expect("singleton is a map")
                    .insert("disc".into(), factored_object_value(discriminant));
            }
            value
        }).collect::<Vec<_>>(),
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
