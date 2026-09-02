//! Structural translation for HyperFLINT's legacy string protocol.
//!
//! The core never represents indexed constants by names such as `mzv_2` or
//! `Wm_1`. Those spellings are accepted and emitted only at string transport
//! boundaries, where they are translated to and from the registered function
//! indeterminates owned by [`super`].

use symbolica::prelude::{Atom, AtomCore, AtomView, ConvertToRing, ParseSettings, Q, Symbol};

use crate::error::{Error, Result};

use super::{SYMBOL_NAMESPACE, algebraic_atoms, heads, log_two_atom, mzv_atom};

/// Append legacy algebraic-letter identifiers for the JSON protocol.
pub(crate) fn append_algebraic_names(
    names: impl IntoIterator<Item = String>,
    pool_size: usize,
) -> Vec<String> {
    let mut output = Vec::new();
    for name in names.into_iter().chain((1..=pool_size).flat_map(|index| {
        [
            format!("Wm_{index}"),
            format!("Wp_{index}"),
            format!("WmOverWp_{index}"),
            format!("sqrt_disc_{index}"),
        ]
    })) {
        if !output.contains(&name) {
            output.push(name);
        }
    }
    output
}

fn signed_integer(atom: AtomView<'_>) -> Option<i64> {
    let AtomView::Num(number) = atom else {
        return None;
    };
    let rational = Q
        .try_element_from_coefficient_view(number.get_coeff_view())
        .ok()?;
    if !rational.is_integer() {
        return None;
    }
    rational.numerator_ref().to_i64()
}

fn positive_index(atom: AtomView<'_>) -> Option<u32> {
    u32::try_from(signed_integer(atom)?)
        .ok()
        .filter(|index| *index > 0)
}

fn algebraic_index(name: &str, prefix: &str) -> Option<u32> {
    name.strip_prefix(prefix)?
        .parse::<u32>()
        .ok()
        .filter(|index| *index > 0)
}

/// Decode one legacy protocol identifier into its registered Atom.
pub(crate) fn special_atom_from_name(name: &str) -> Option<Atom> {
    if name == "Log2" {
        return Some(log_two_atom());
    }
    if let Some(rest) = name.strip_prefix("mzv_") {
        if rest.is_empty() {
            return None;
        }
        let indices = rest
            .split('_')
            .map(|part| {
                let (negative, digits) = match part.strip_prefix('m') {
                    Some(digits) => (true, digits),
                    None => (false, part),
                };
                let value = digits.parse::<i64>().ok()?;
                Some(if negative { -value } else { value })
            })
            .collect::<Option<Vec<_>>>()?;
        return Some(mzv_atom(&indices));
    }
    if let Some(index) = algebraic_index(name, "Wm_") {
        return Some(algebraic_atoms(index).minus);
    }
    if let Some(index) = algebraic_index(name, "Wp_") {
        return Some(algebraic_atoms(index).plus);
    }
    if let Some(index) = algebraic_index(name, "WmOverWp_") {
        return Some(algebraic_atoms(index).ratio);
    }
    algebraic_index(name, "sqrt_disc_").map(|index| algebraic_atoms(index).sqrt_discriminant)
}

/// Encode one registered special indeterminate in legacy protocol notation.
pub(crate) fn special_name_from_atom(atom: AtomView<'_>) -> Option<String> {
    let symbols = heads();
    if atom == log_two_atom().as_view() {
        return Some("Log2".to_owned());
    }
    let function = atom.as_fun_view()?;
    if function.get_symbol() == symbols.mzv {
        let indices = function
            .iter()
            .map(signed_integer)
            .collect::<Option<Vec<_>>>()?;
        if indices.is_empty() {
            return None;
        }
        let suffix = indices
            .into_iter()
            .map(|index| {
                if index < 0 {
                    format!("m{}", index.unsigned_abs())
                } else {
                    index.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("_");
        return Some(format!("mzv_{suffix}"));
    }
    if function.get_nargs() != 1 {
        return None;
    }
    let index = positive_index(function.get(0))?;
    let prefix = if function.get_symbol() == symbols.algebraic_minus {
        "Wm_"
    } else if function.get_symbol() == symbols.algebraic_plus {
        "Wp_"
    } else if function.get_symbol() == symbols.algebraic_ratio {
        "WmOverWp_"
    } else if function.get_symbol() == symbols.sqrt_discriminant {
        "sqrt_disc_"
    } else {
        return None;
    };
    Some(format!("{prefix}{index}"))
}

#[cfg(test)]
fn wire_symbol(name: &str) -> Atom {
    Symbol::parse(name, SYMBOL_NAMESPACE)
        .expect("validated legacy identifier must parse as a Symbolica symbol")
        .to_atom()
}

/// Replace legacy protocol variables by registered function indeterminates.
pub(crate) fn import_special_atoms(atom: impl AtomCore) -> Atom {
    atom.as_atom_view().replace_map(|term, _, output| {
        let AtomView::Var(variable) = term else {
            return;
        };
        let symbol = variable.get_symbol();
        let full_name = symbol.get_name();
        let short_name = full_name.rsplit("::").next().unwrap_or(full_name);
        if let Some(replacement) = special_atom_from_name(short_name) {
            **output = replacement;
        }
    })
}

/// Replace registered special indeterminates by legacy protocol variables.
#[cfg(test)]
pub(crate) fn export_special_atoms(atom: impl AtomCore) -> Atom {
    atom.as_atom_view().replace_map(|term, _, output| {
        if let Some(name) = special_name_from_atom(term) {
            **output = wire_symbol(&name);
        }
    })
}

/// Parse a legacy transport expression and translate it structurally.
pub(crate) fn parse_expression(expression: &str) -> Result<Atom> {
    let atom =
        Atom::parse(expression, SYMBOL_NAMESPACE, ParseSettings::default()).map_err(|message| {
            Error::RationalParse {
                expression: expression.to_owned(),
                message,
            }
        })?;
    Ok(import_special_atoms(atom))
}

/// Format a native expression in byte-compatible legacy identifier notation.
#[cfg(test)]
pub(crate) fn format_expression(atom: impl AtomCore) -> String {
    super::plain_atom_string(export_special_atoms(atom))
}

/// Convert one legacy variable name, leaving ordinary names as plain symbols.
pub(crate) fn atom_from_name(name: &str) -> Result<Atom> {
    special_atom_from_name(name).map_or_else(
        || {
            if let Some((head, subscripts)) = name.split_once('[')
                && !head.is_empty()
                && name.ends_with(']')
            {
                let body = &subscripts[..subscripts.len() - 1];
                let indices = body
                    .split(',')
                    .map(str::trim)
                    .map(str::parse::<i64>)
                    .collect::<std::result::Result<Vec<_>, _>>();
                if let Ok(indices) = indices
                    && !indices.is_empty()
                {
                    return Symbol::parse(head, SYMBOL_NAMESPACE)
                        .map(|symbol| symbol.call_args(indices))
                        .map_err(|message| Error::PolynomialParse {
                            expression: name.to_owned(),
                            message,
                        });
                }
            }
            Symbol::parse(name, SYMBOL_NAMESPACE)
                .map(Symbol::to_atom)
                .map_err(|message| Error::PolynomialParse {
                    expression: name.to_owned(),
                    message,
                })
        },
        Ok,
    )
}

#[cfg(test)]
mod tests {
    use symbolica::prelude::{AtomCore, function, symbol};

    use super::*;

    #[test]
    fn legacy_specials_round_trip_structurally() {
        let imported = parse_expression("mzv_m2_3*Log2+Wm_1/Wp_2+WmOverWp_3+sqrt_disc_4").unwrap();
        assert!(imported.contains_symbol(heads().mzv));
        assert!(imported.contains_symbol(heads().algebraic_minus));
        assert!(!imported.get_all_symbols(true).iter().any(|symbol| {
            let name = symbol.get_name();
            name.contains("mzv_") || name.contains("Wm_") || name.contains("sqrt_disc_")
        }));
        assert_eq!(
            parse_expression(&format_expression(&imported)).unwrap(),
            imported
        );
    }

    #[test]
    fn unrelated_function_indeterminates_survive_both_directions() {
        let f = symbol!("legacy_adapter_f");
        let call = function!(f, 7);
        let imported = import_special_atoms(&call);
        assert_eq!(imported, call);
        assert_eq!(export_special_atoms(&call), call);
    }

    #[test]
    fn legacy_algebraic_pool_has_stable_wire_order() {
        assert_eq!(
            append_algebraic_names(vec!["x".into(), "Wm_1".into()], 2),
            [
                "x",
                "Wm_1",
                "Wp_1",
                "WmOverWp_1",
                "sqrt_disc_1",
                "Wm_2",
                "Wp_2",
                "WmOverWp_2",
                "sqrt_disc_2",
            ]
        );
    }
}
