use std::sync::Arc;

use serde_json::Value;

use super::super::wire::{array_field, parse_wire_poly, string_array_field};
use crate::core::{Poly, PolyCtx};
use crate::error::{Error, Result};
use crate::integrator::lr_scan::{KeepRule, ScanExponent, ScanOptions};

type LrInput = (Arc<PolyCtx>, Vec<String>, Vec<Vec<Poly>>, Vec<usize>);

pub(super) fn lr_input(request: &Value) -> Result<LrInput> {
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

pub(super) fn scan_exponents(request: &Value) -> Result<Vec<Vec<ScanExponent>>> {
    array_field(request, "exps")?
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
        .collect()
}

pub(super) fn scan_options(request: &Value) -> Result<ScanOptions> {
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
    Ok(ScanOptions {
        keep_rule,
        euler_filter: super::super::wire::optional_bool(request, "euler_filter", false)?,
        max_orders: super::super::wire::optional_usize(request, "max_orders", 8192)?,
    })
}

pub(super) fn legacy_euler_environment() -> bool {
    std::env::var("HF_EULER_FILTER").is_ok_and(|value| !value.is_empty() && value != "0")
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
