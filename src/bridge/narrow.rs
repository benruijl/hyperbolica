//! Shared request-payload discovery for narrow standard-MZV contexts.

use serde_json::Value;
use symbolica::prelude::Atom;

use crate::error::{Error, Result};
use crate::reduce::{
    MzvReductionTable, build_mzv_atom_list, build_narrow_var_list, mzv_constant_atom,
};
use crate::symbols::legacy;

/// Borrow every string nested below the selected top-level payload fields.
///
/// Coefficients, word letters, regulator keys, and symbolic-coefficient
/// prefactors live at different JSON depths. Keeping the recursive walk here
/// prevents each bridge operation from maintaining its own partial schema
/// scanner solely for context narrowing.
pub(super) fn payload_strings<'a>(request: &'a Value, fields: &[&str]) -> Vec<&'a str> {
    fn collect<'a>(value: &'a Value, output: &mut Vec<&'a str>) {
        match value {
            Value::String(value) => output.push(value),
            Value::Array(values) => {
                for value in values {
                    collect(value, output);
                }
            }
            Value::Object(values) => {
                for value in values.values() {
                    collect(value, output);
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }

    let mut output = Vec::new();
    for field in fields {
        if let Some(value) = request.get(field) {
            collect(value, &mut output);
        }
    }
    output
}

/// Complete user atoms with exactly the standard MZVs reachable from the
/// current payload, or with the full table for an explicit custom table.
pub(super) fn mzv_indeterminates(
    table: &MzvReductionTable,
    mut user_indeterminates: Vec<Atom>,
    expressions: &[&str],
) -> Result<Vec<Atom>> {
    if !table.is_embedded_standard() {
        return build_mzv_atom_list(table, user_indeterminates);
    }

    let user_mzv_names = user_indeterminates
        .iter()
        .filter_map(|atom| legacy::special_name_from_atom(atom.as_view()))
        .filter(|name| mzv_constant_atom(name).is_some())
        .collect::<Vec<_>>();
    let combined = expressions.join("+");
    for name in build_narrow_var_list(table, &user_mzv_names, &combined) {
        let atom = mzv_constant_atom(&name).ok_or_else(|| {
            Error::InvalidInput(format!("invalid MZV reduction-table identifier `{name}`"))
        })?;
        if !user_indeterminates.contains(&atom) {
            user_indeterminates.push(atom);
        }
    }
    Ok(user_indeterminates)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::reduce::standard_mzv_reductions;
    use crate::symbols::algebraic_atoms;

    #[test]
    fn payload_string_walk_covers_nested_wire_shapes_only_in_selected_fields() {
        let request = json!({
            "ignored": "outside",
            "regulator": [{
                "coef": "mzv_4",
                "key": [["1/(1+x)", "mzv_6"]],
                "metadata": {"delta": "mzv_8"},
            }],
        });
        let strings = payload_strings(&request, &["regulator"]);
        assert!(strings.contains(&"mzv_4"));
        assert!(strings.contains(&"1/(1+x)"));
        assert!(strings.contains(&"mzv_6"));
        assert!(strings.contains(&"mzv_8"));
        assert!(!strings.contains(&"outside"));
    }

    #[test]
    fn algebraic_atoms_remain_user_indeterminates_not_mzv_identifiers() {
        let algebraic = algebraic_atoms(1).minus;
        let narrowed =
            mzv_indeterminates(&standard_mzv_reductions(), vec![algebraic.clone()], &[]).unwrap();
        assert!(narrowed.contains(&algebraic));
    }
}
