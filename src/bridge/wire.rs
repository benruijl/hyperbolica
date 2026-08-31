//! Shared JSON request validation and response encoding.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde_json::{Value, json};
use symbolica::prelude::{Atom, AtomCore, Rational};

use crate::core::{Poly, PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::integrator::{RegTerm, RegTermSym, Regulator, RegulatorSym};
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
    let mut indeterminates = BTreeSet::new();
    for expression in expressions {
        let atom = legacy::parse_expression(expression)?;
        indeterminates.extend(
            atom.get_all_indeterminates(false)
                .into_iter()
                .map(|indeterminate| indeterminate.to_owned()),
        );
    }
    Ok(indeterminates.into_iter().collect())
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
        .map(|index| {
            let atom = ctx
                .variable_atom(index)
                .expect("context variable index is in bounds");
            legacy::special_name_from_atom(atom.as_view()).unwrap_or_else(|| atom.to_string())
        })
        .collect()
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
    Value::Array(
        regulator
            .iter()
            .map(|term| {
                json!({
                    "coef": wire_rat(&term.coef),
                    "key": term.key.iter().map(|word| {
                        word.letters.iter().map(wire_rat).collect::<Vec<_>>()
                    }).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

pub(super) fn regulator_sym_value(regulator: &RegulatorSym) -> Value {
    Value::Array(
        regulator
            .iter()
            .map(|term: &RegTermSym| {
                json!({
                    "coef": super::symcoef::symcoef_string(&term.coef),
                    "key": term.key.iter().map(|word| {
                        word.letters.iter().map(wire_rat).collect::<Vec<_>>()
                    }).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}
