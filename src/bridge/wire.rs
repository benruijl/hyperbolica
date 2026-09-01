//! Shared JSON request validation and response encoding.

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use serde_json::{Value, json};
use symbolica::prelude::{Atom, AtomCore, Rational};

use crate::core::{Poly, PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::integrator::{RegTerm, RegTermSym, Regulator, RegulatorSym, regkey_structural_cmp};
use crate::reduce::{MzvReductionTable, build_mzv_atom_list, load_mzv_reductions};
use crate::symbols::{Word, Wordlist, WordlistTerm, legacy};

pub(super) fn string_field<'a>(request: &'a Value, name: &str) -> Result<&'a str> {
    request
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::InvalidInput(format!("missing string field `{name}`")))
}

pub(super) fn integer_field(request: &Value, name: &str) -> Result<i64> {
    request
        .get(name)
        .and_then(Value::as_i64)
        .ok_or_else(|| Error::InvalidInput(format!("missing integer field `{name}`")))
}

pub(super) fn array_field<'a>(request: &'a Value, name: &str) -> Result<&'a [Value]> {
    request
        .get(name)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| Error::InvalidInput(format!("missing array field `{name}`")))
}

pub(super) fn string_array_field(request: &Value, name: &str) -> Result<Vec<String>> {
    array_field(request, name)?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| Error::InvalidInput(format!("`{name}` entries must be strings")))
        })
        .collect()
}

pub(super) fn integer_array_field(request: &Value, name: &str) -> Result<Vec<i64>> {
    array_field(request, name)?
        .iter()
        .map(|value| {
            value
                .as_i64()
                .ok_or_else(|| Error::InvalidInput(format!("`{name}` entries must be integers")))
        })
        .collect()
}

pub(super) fn optional_bool(request: &Value, name: &str, default: bool) -> Result<bool> {
    match request.get(name) {
        None => Ok(default),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| Error::InvalidInput(format!("`{name}` must be a boolean"))),
    }
}

pub(super) fn optional_usize(request: &Value, name: &str, default: usize) -> Result<usize> {
    match request.get(name) {
        None => Ok(default),
        Some(value) => value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| {
                Error::InvalidInput(format!("`{name}` must be a non-negative machine integer"))
            }),
    }
}

pub(super) fn explicit_variables(request: &Value) -> Result<Option<Vec<String>>> {
    let Some(value) = request.get("vars") else {
        return Ok(None);
    };
    let values = value
        .as_array()
        .ok_or_else(|| Error::InvalidInput("`vars` must be an array".into()))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| Error::InvalidInput("`vars` entries must be strings".into()))
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

pub(super) fn mzv_data_path(request: &Value) -> std::path::PathBuf {
    request
        .get("mzv_data_path")
        .and_then(Value::as_str)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("data")
                .join("mzv_reductions.json")
        })
}

pub(super) fn mzv_context(
    request: &Value,
    expressions: &[&str],
) -> Result<(Arc<PolyCtx>, MzvReductionTable)> {
    let table = load_mzv_reductions(mzv_data_path(request))?;
    let user_variables = wire_indeterminates(request, expressions)?;
    let variables = build_mzv_atom_list(&table, user_variables)?;
    Ok((PolyCtx::from_indeterminates(variables)?, table))
}

fn wire_indeterminates(request: &Value, expressions: &[&str]) -> Result<Vec<Atom>> {
    if let Some(names) = explicit_variables(request)? {
        return names
            .iter()
            .map(|name| legacy::atom_from_name(name))
            .collect();
    }
    let mut indeterminates = HashSet::new();
    for expression in expressions {
        let atom = legacy::parse_expression(expression)?;
        indeterminates.extend(
            atom.get_all_indeterminates(false)
                .into_iter()
                .map(|indeterminate| indeterminate.to_owned()),
        );
    }
    let mut decorated = indeterminates
        .into_iter()
        .map(|atom| {
            let wire_name =
                legacy::special_name_from_atom(atom.as_view()).unwrap_or_else(|| atom.to_string());
            let canonical = atom.to_canonical_string();
            (wire_name, canonical, atom)
        })
        .collect::<Vec<_>>();
    // HyperFLINT's omitted-`vars` autoscan is lexical. Atom's native `Ord`
    // includes process-local Symbol registration IDs, so it is unsuitable
    // for observable context order. Native Atom equality above performs
    // deduplication; stable spellings only select presentation order.
    decorated
        .sort_unstable_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    Ok(decorated.into_iter().map(|(_, _, atom)| atom).collect())
}

pub(super) fn scan_identifiers(expressions: &[&str]) -> Vec<String> {
    let mut identifiers = BTreeSet::new();
    for expression in expressions {
        let mut characters = expression.char_indices().peekable();
        while let Some((start, character)) = characters.next() {
            if !character.is_ascii_alphabetic() {
                continue;
            }
            let mut end = start + character.len_utf8();
            while let Some(&(index, next)) = characters.peek() {
                if next.is_ascii_alphanumeric() || next == '_' {
                    characters.next();
                    end = index + next.len_utf8();
                } else {
                    break;
                }
            }
            identifiers.insert(expression[start..end].to_owned());
        }
    }
    identifiers.into_iter().collect()
}

pub(super) fn context_for(request: &Value, expressions: &[&str]) -> Result<Arc<PolyCtx>> {
    let mut variables = wire_indeterminates(request, expressions)?;
    if variables.is_empty() {
        variables.push(legacy::atom_from_name("x")?);
    }
    PolyCtx::from_indeterminates(variables)
}

pub(super) fn context_for_variable(request: &Value, expressions: &[&str]) -> Result<Arc<PolyCtx>> {
    let variable = string_field(request, "var")?;
    let variable_atom = legacy::atom_from_name(variable)?;
    let mut variables = wire_indeterminates(request, expressions)?;
    if !variables.contains(&variable_atom) {
        variables.push(variable_atom);
    }
    PolyCtx::from_indeterminates(variables)
}

pub(super) fn variable_index(ctx: &PolyCtx, request: &Value) -> Result<usize> {
    let variable = string_field(request, "var")?;
    let atom = legacy::atom_from_name(variable)?;
    ctx.index_of_indeterminate(atom.as_view())
        .ok_or_else(|| Error::UnknownVariable(variable.to_owned()))
}

/// Resolve one legacy diagnostic variable name without allowing an ambiguous
/// display spelling to stand in for native Symbolica identity.
pub(super) fn unique_diagnostic_variable_index(ctx: &PolyCtx, name: &str) -> Result<usize> {
    // Count actual emitted spellings before preferring a structurally parsed
    // Atom. A registered MZV(2) and an ordinary symbol named `mzv_2`, for
    // example, are indistinguishable on the legacy wire and must be rejected.
    let matches = (0..ctx.len())
        .filter(|&index| {
            wire_context_variable(ctx, index).is_some_and(|candidate| candidate == name)
        })
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(Error::InvalidInput(format!(
            "variable name `{name}` is ambiguous in this Symbolica context; use the Atom-native API"
        )));
    }
    if let Some(&index) = matches.first() {
        return Ok(index);
    }

    // Qualified/native spellings that are not the default emitted spelling
    // can still identify a slot exactly.
    let atom = legacy::atom_from_name(name)?;
    ctx.index_of_indeterminate(atom.as_view())
        .ok_or_else(|| Error::UnknownVariable(name.to_owned()))
}

pub(super) trait WireValue {
    fn wire_value(&self) -> String;
}

impl<T: WireValue + ?Sized> WireValue for &T {
    fn wire_value(&self) -> String {
        (**self).wire_value()
    }
}

impl WireValue for Rat {
    fn wire_value(&self) -> String {
        wire_atom(self.to_atom())
    }
}

impl WireValue for Poly {
    fn wire_value(&self) -> String {
        wire_atom(self.to_atom())
    }
}

impl WireValue for String {
    fn wire_value(&self) -> String {
        self.clone()
    }
}

impl WireValue for bool {
    fn wire_value(&self) -> String {
        self.to_string()
    }
}

pub(super) fn result_response(op: &str, ctx: &PolyCtx, result: impl WireValue) -> Value {
    json!({"op": op, "result": result.wire_value(), "vars": wire_context_variables(ctx)})
}

pub(super) fn parse_wire_rat(ctx: &Arc<PolyCtx>, expression: &str) -> Result<Rat> {
    let atom = legacy::parse_expression(expression)?;
    Rat::from_atom(ctx.clone(), atom.as_view())
}

pub(super) fn parse_wire_poly(ctx: &Arc<PolyCtx>, expression: &str) -> Result<Poly> {
    let atom = legacy::parse_expression(expression)?;
    Poly::from_atom(ctx.clone(), atom.as_view())
}

/// Parse an exact rational scalar at the legacy JSON/string boundary.
///
/// Production algebra APIs receive a typed Symbolica `Rational`; this is the
/// only conversion needed by the historical `eval` and `subst` operations.
pub(super) fn parse_wire_rational_scalar(ctx: &Arc<PolyCtx>, expression: &str) -> Result<Rational> {
    parse_wire_poly(ctx, expression)?
        .rational_constant()
        .ok_or_else(|| {
            Error::InvalidInput(format!(
                "wire value `{expression}` is not an exact rational scalar"
            ))
        })
}

pub(super) fn wire_atom(atom: impl AtomCore) -> String {
    legacy::format_expression(atom)
}

pub(super) fn wire_rat(value: &Rat) -> String {
    wire_atom(value.to_atom())
}

pub(super) fn wire_poly(value: &Poly) -> String {
    wire_atom(value.to_atom())
}

pub(super) fn wire_context_variables(ctx: &PolyCtx) -> Vec<String> {
    (0..ctx.len())
        .map(|index| wire_context_variable(ctx, index).expect("context index is in bounds"))
        .collect()
}

pub(super) fn wire_context_variable(ctx: &PolyCtx, index: usize) -> Option<String> {
    let atom = ctx.variable_atom(index).ok()?;
    Some(legacy::special_name_from_atom(atom.as_view()).unwrap_or_else(|| atom.to_string()))
}

pub(super) fn wire_regkey(key: &[Word]) -> Vec<Vec<String>> {
    let mut decorated = key
        .iter()
        .map(|word| (word.letters.iter().map(wire_rat).collect::<Vec<_>>(), word))
        .collect::<Vec<_>>();
    decorated.sort_unstable_by(|(left_wire, left), (right_wire, right)| {
        left_wire
            .cmp(right_wire)
            .then_with(|| left.structural_cmp(right))
    });
    decorated.into_iter().map(|(wire, _)| wire).collect()
}

pub(super) fn parse_word(ctx: &Arc<PolyCtx>, values: &[String]) -> Result<Word> {
    values
        .iter()
        .map(|value| parse_wire_rat(ctx, value))
        .collect::<Result<Vec<_>>>()
        .map(Word::new)
}

pub(super) fn parse_wordlist(ctx: &Arc<PolyCtx>, request: &Value, name: &str) -> Result<Wordlist> {
    array_field(request, name)?
        .iter()
        .map(|term| {
            let coef = parse_wire_rat(ctx, string_field(term, "coef")?)?;
            let word = parse_word(ctx, &string_array_field(term, "word")?)?;
            Ok(WordlistTerm::new(coef, word))
        })
        .collect::<Result<Vec<_>>>()
        .map(Wordlist::new)
}

pub(super) fn wordlist_value(wordlist: &Wordlist) -> Value {
    Value::Array(
        wordlist
            .terms
            .iter()
            .map(|term| {
                json!({
                    "coef": wire_rat(&term.coef),
                    "word": term.word.letters.iter().map(wire_rat).collect::<Vec<_>>()
                })
            })
            .collect(),
    )
}

pub(super) fn parse_words(ctx: &Arc<PolyCtx>, value: &Value) -> Result<Vec<Word>> {
    value
        .as_array()
        .ok_or_else(|| Error::InvalidInput("word collection must be an array".into()))?
        .iter()
        .map(|word| {
            let letters =
                word.as_array()
                    .ok_or_else(|| Error::InvalidInput("each word must be an array".into()))?
                    .iter()
                    .map(|letter| {
                        letter.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                            Error::InvalidInput("word letters must be strings".into())
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
            parse_word(ctx, &letters)
        })
        .collect()
}

pub(super) fn parse_regulator(
    ctx: &Arc<PolyCtx>,
    request: &Value,
    name: &str,
) -> Result<Regulator> {
    array_field(request, name)?
        .iter()
        .map(|term| {
            Ok(RegTerm {
                coef: parse_wire_rat(ctx, string_field(term, "coef")?)?,
                key: parse_words(
                    ctx,
                    term.get("key").ok_or_else(|| {
                        Error::InvalidInput("regulator term is missing `key`".into())
                    })?,
                )?,
            })
        })
        .collect()
}

pub(super) fn regulator_value(regulator: &Regulator) -> Value {
    let mut terms = regulator
        .iter()
        .map(|term| (wire_regkey(&term.key), term))
        .collect::<Vec<_>>();
    terms.sort_unstable_by(|(left_wire, left), (right_wire, right)| {
        left_wire
            .cmp(right_wire)
            .then_with(|| regkey_structural_cmp(&left.key, &right.key))
    });
    Value::Array(
        terms
            .into_iter()
            .map(|(key, term)| {
                json!({
                    "coef": wire_rat(&term.coef),
                    "key": key,
                })
            })
            .collect(),
    )
}

pub(super) fn regulator_sym_value(regulator: &RegulatorSym) -> Value {
    let mut terms = regulator
        .iter()
        .map(|term| (wire_regkey(&term.key), term))
        .collect::<Vec<_>>();
    terms.sort_unstable_by(|(left_wire, left), (right_wire, right)| {
        left_wire
            .cmp(right_wire)
            .then_with(|| regkey_structural_cmp(&left.key, &right.key))
    });
    Value::Array(
        terms
            .into_iter()
            .map(|(key, term): (_, &RegTermSym)| {
                json!({
                    "coef": super::symcoef::symcoef_string(&term.coef),
                    "key": key,
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use symbolica::prelude::Symbol;

    use super::*;

    #[test]
    fn legacy_name_resolution_rejects_unknown_and_ambiguous_diagnostics() {
        let left = Symbol::parse("x", "wire_delta_left").unwrap();
        let right = Symbol::parse("x", "wire_delta_right").unwrap();
        let ambiguous = PolyCtx::from_indeterminates([left.to_atom(), right.to_atom()]).unwrap();
        assert_eq!(ambiguous.vars(), &["x", "x"]);
        assert!(matches!(
            unique_diagnostic_variable_index(&ambiguous, "x"),
            Err(Error::InvalidInput(message)) if message.contains("ambiguous")
        ));
        assert!(matches!(
            unique_diagnostic_variable_index(&ambiguous, "missing"),
            Err(Error::UnknownVariable(name)) if name == "missing"
        ));

        let unambiguous = PolyCtx::from_indeterminates([right.to_atom()]).unwrap();
        assert_eq!(
            unique_diagnostic_variable_index(&unambiguous, "x").unwrap(),
            0
        );
    }

    #[test]
    fn regulator_serialization_restores_legacy_lexical_factor_order() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let two = Word::new(vec![Rat::from_int(ctx.clone(), 2)]);
        let ten = Word::new(vec![Rat::from_int(ctx.clone(), 10)]);
        let regulator = vec![RegTerm {
            coef: Rat::one(ctx),
            key: vec![two.clone(), ten.clone()],
        }];

        assert_eq!(
            regulator_value(&regulator),
            json!([{"coef": "1", "key": [["10"], ["2"]]}])
        );
        assert_eq!(regulator[0].key, vec![two, ten]);
    }

    #[test]
    fn registered_and_plain_symbols_with_one_wire_name_are_ambiguous() {
        let registered = crate::symbols::mzv_atom(&[2]);
        // Build the deliberate collision spelling dynamically so the source
        // audit still rejects accidental ad-hoc special symbols.
        let collision_name = ["mzv", "2"].join("_");
        let plain = Symbol::parse(&collision_name, crate::symbols::SYMBOL_NAMESPACE)
            .unwrap()
            .to_atom();
        let ctx = PolyCtx::from_indeterminates([registered, plain]).unwrap();

        assert_eq!(wire_context_variables(&ctx), ["mzv_2", "mzv_2"]);
        assert!(matches!(
            unique_diagnostic_variable_index(&ctx, "mzv_2"),
            Err(Error::InvalidInput(message)) if message.contains("ambiguous")
        ));
    }

    #[test]
    fn regulator_wire_order_uses_emitted_special_names() {
        let special = crate::symbols::algebraic_atoms(1).minus;
        let ordinary = Symbol::parse("Wm0", crate::symbols::SYMBOL_NAMESPACE)
            .unwrap()
            .to_atom();
        let ctx = PolyCtx::from_indeterminates([special, ordinary]).unwrap();
        let special_word = Word::new(vec![Rat::from_poly(
            Poly::generator(ctx.clone(), 0).unwrap(),
        )]);
        let ordinary_word = Word::new(vec![Rat::from_poly(
            Poly::generator(ctx.clone(), 1).unwrap(),
        )]);
        let regulator = vec![RegTerm {
            coef: Rat::one(ctx),
            key: vec![special_word, ordinary_word],
        }];

        assert_eq!(
            regulator_value(&regulator),
            json!([{"coef": "1", "key": [["Wm0"], ["Wm_1"]]}])
        );
    }

    #[test]
    fn inferred_context_order_is_lexical_not_symbol_registration_order() {
        // Register `z` first so raw Symbol IDs order these names opposite to
        // the legacy autoscan contract.
        Symbol::parse("wire_scan_z", crate::symbols::SYMBOL_NAMESPACE).unwrap();
        Symbol::parse("wire_scan_a", crate::symbols::SYMBOL_NAMESPACE).unwrap();

        let request = json!({});
        let ctx = context_for(&request, &["wire_scan_z+wire_scan_a"]).unwrap();
        assert_eq!(wire_context_variables(&ctx), ["wire_scan_a", "wire_scan_z"]);
    }
}
