//! Shared JSON request validation and response encoding.

use std::collections::{BTreeSet, HashSet};
use std::fmt::{Display, Write};
use std::sync::Arc;

use serde_json::{Value, json};
use symbolica::prelude::{
    Atom, AtomCore, Exponent, MonomialOrder, MultivariatePolynomial, Rational, Ring,
};

use crate::core::{Poly, PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::integrator::{RegTerm, RegTermSym, Regulator, RegulatorSym, regkey_structural_cmp};
use crate::reduce::MzvReductionTable;
use crate::symbols::{Word, Wordlist, WordlistTerm, legacy, plain_atom_string};

use super::mzv_data::mzv_reduction_table;
use super::narrow::mzv_indeterminates;

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
    match request.get(name) {
        None => Ok(&[]),
        Some(Value::Array(values)) => Ok(values),
        Some(_) => Err(Error::InvalidInput(format!("`{name}` must be an array"))),
    }
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
    // The upstream flat-JSON adapter recognizes only literal true/false.
    // Missing fields and values of another JSON type both retain the
    // operation's default.
    Ok(request
        .get(name)
        .and_then(Value::as_bool)
        .unwrap_or(default))
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
    let variables = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| Error::InvalidInput("`vars` entries must be strings".into()))
        })
        .collect::<Result<Vec<_>>>()?;
    // HyperFLINT treats an explicitly empty list like an omitted list and
    // falls back to lexical variable discovery.
    Ok((!variables.is_empty()).then_some(variables))
}

pub(super) fn mzv_context(
    request: &Value,
    expressions: &[&str],
) -> Result<(Arc<PolyCtx>, MzvReductionTable)> {
    let table = mzv_reduction_table(request)?;
    let user_variables = wire_indeterminates(request, expressions)?;
    let variables = mzv_indeterminates(&table, user_variables, expressions)?;
    Ok((PolyCtx::from_indeterminates(variables)?, table))
}

pub(super) fn mzv_context_for_variable(
    request: &Value,
    expressions: &[&str],
) -> Result<(Arc<PolyCtx>, MzvReductionTable)> {
    let table = mzv_reduction_table(request)?;
    let variable = string_field(request, "var")?;
    let variable_atom = legacy::atom_from_name(variable)?;
    let mut user_variables = wire_indeterminates(request, expressions)?;
    if !user_variables.contains(&variable_atom) {
        user_variables.push(variable_atom);
    }
    let variables = mzv_indeterminates(&table, user_variables, expressions)?;
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
            let wire_name = legacy::special_name_from_atom(atom.as_view())
                .unwrap_or_else(|| plain_atom_string(&atom));
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
        wire_rat(self)
    }
}

impl WireValue for Poly {
    fn wire_value(&self) -> String {
        wire_poly(self)
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

pub(super) fn wire_rat(value: &Rat) -> String {
    let variables = wire_context_variables(value.ctx());
    let numerator = pretty_wire_polynomial(&value.native().numerator, &variables);
    if value.native().denominator.is_one() {
        numerator
    } else {
        let denominator = pretty_wire_polynomial(&value.native().denominator, &variables);
        format!(
            "{}/{}",
            wrap_wire_numerator(numerator),
            wrap_wire_denominator(denominator)
        )
    }
}

pub(super) fn wire_poly(value: &Poly) -> String {
    pretty_wire_polynomial(value.inner(), &wire_context_variables(value.ctx()))
}

fn pretty_wire_polynomial<F, E, O>(
    value: &MultivariatePolynomial<F, E, O>,
    variables: &[String],
) -> String
where
    F: Ring,
    F::Element: Display,
    E: Exponent,
    O: MonomialOrder,
{
    if value.nterms() == 0 {
        return "0".into();
    }

    let mut output = String::new();
    for term_index in (0..value.nterms()).rev() {
        let coefficient = value.coefficients[term_index].to_string();
        let (negative, magnitude) = coefficient
            .strip_prefix('-')
            .map_or((false, coefficient.as_str()), |magnitude| (true, magnitude));
        if term_index == value.nterms() - 1 {
            if negative {
                output.push('-');
            }
        } else if negative {
            output.push_str(" - ");
        } else {
            output.push_str(" + ");
        }

        let exponents = value.exponents(term_index);
        let has_monomial = exponents.iter().any(|exponent| !exponent.is_zero());
        if !has_monomial || magnitude != "1" {
            output.push_str(magnitude);
            if has_monomial {
                output.push('*');
            }
        }

        let mut first_variable = true;
        for (name, exponent) in variables.iter().zip(exponents) {
            if exponent.is_zero() {
                continue;
            }
            if !first_variable {
                output.push('*');
            }
            first_variable = false;
            output.push_str(name);
            if exponent.to_i32() != 1 {
                write!(output, "^{exponent}").expect("writing to a String cannot fail");
            }
        }
    }
    output
}

fn wire_has_top_level(expression: &str, needle: char) -> bool {
    let mut depth = 0_i32;
    for character in expression.chars().skip(1) {
        match character {
            '(' => depth += 1,
            ')' => depth -= 1,
            character if depth == 0 && character == needle => return true,
            _ => {}
        }
    }
    false
}

fn wrap_wire_numerator(expression: String) -> String {
    if ['+', '-', ' ']
        .into_iter()
        .any(|needle| wire_has_top_level(&expression, needle))
    {
        format!("({expression})")
    } else {
        expression
    }
}

fn wrap_wire_denominator(expression: String) -> String {
    if ['+', '-', '*', ' ']
        .into_iter()
        .any(|needle| wire_has_top_level(&expression, needle))
    {
        format!("({expression})")
    } else {
        expression
    }
}

pub(super) fn wire_context_variables(ctx: &PolyCtx) -> Vec<String> {
    (0..ctx.len())
        .map(|index| wire_context_variable(ctx, index).expect("context index is in bounds"))
        .collect()
}

pub(super) fn wire_context_variable(ctx: &PolyCtx, index: usize) -> Option<String> {
    let atom = ctx.variable_atom(index).ok()?;
    Some(legacy::special_name_from_atom(atom.as_view()).unwrap_or_else(|| plain_atom_string(&atom)))
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
mod tests;
