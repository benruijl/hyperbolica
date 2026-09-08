//! Eager expansion of generated MZV identities into a basis-only context.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, OnceLock};

use symbolica::domains::rational_polynomial::FromNumeratorAndDenominator;
use symbolica::prelude::{Atom, Z};

use crate::core::{NativeRat, PolyCtx, Rat};
use crate::error::{Error, Result};

use super::mzv_reduce::{
    MzvReductionRule, MzvReductionTable, identifier_tokens, load_mzv_reductions, mzv_constant_atom,
    mzv_expression_atom, standard_mzv_reductions, substitute_var_rat,
};

mod lazy;
pub(crate) use lazy::{ExpansionSource, StandardMzvExpansion, standard_mzv_expansion_lazy};

#[derive(Clone, Debug)]
pub struct MzvExpansionTable {
    pub basis_ctx: Arc<PolyCtx>,
    /// Registered basis indeterminates in the same order as `basis_names`.
    pub basis_atoms: Vec<Atom>,
    /// Legacy generated-table identifiers retained only for table lookup and
    /// compatibility serialization.
    pub basis_names: Vec<String>,
    pub basis_idx: HashMap<String, usize>,
    /// Expanded rules in lexicographic order for deterministic inspection.
    pub expansion: BTreeMap<String, Rat>,
}

/// Transcendental weight encoded by an MZV symbol, or `-1` when invalid.
pub fn weight_of_mzv_name(name: &str) -> i32 {
    if name == "Log2" {
        return 1;
    }
    let Some(mut rest) = name.strip_prefix("mzv_") else {
        return -1;
    };
    if rest.is_empty() {
        return -1;
    }

    let mut weight = 0_i32;
    loop {
        if let Some(stripped) = rest.strip_prefix('m') {
            rest = stripped;
        }
        let digit_count = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digit_count == 0 {
            return -1;
        }
        let Ok(value) = rest[..digit_count].parse::<i32>() else {
            return -1;
        };
        let Some(next_weight) = weight.checked_add(value) else {
            return -1;
        };
        weight = next_weight;
        rest = &rest[digit_count..];
        if rest.is_empty() {
            return weight;
        }
        let Some(stripped) = rest.strip_prefix('_') else {
            return -1;
        };
        if stripped.is_empty() {
            return -1;
        }
        rest = stripped;
    }
}

pub fn looks_like_mzv(token: &str) -> bool {
    token == "Log2" || weight_of_mzv_name(token) >= 1
}

pub fn tokens_in(expression: &str) -> Vec<String> {
    identifier_tokens(expression)
}

/// Append the basis as one contiguous tail block, preserving input order.
pub fn build_basis_var_list(
    expansion: &MzvExpansionTable,
    user_variables: &[String],
) -> Vec<String> {
    let mut output = user_variables.to_vec();
    let mut seen = output.iter().cloned().collect::<HashSet<_>>();
    for basis in &expansion.basis_names {
        if seen.insert(basis.clone()) {
            output.push(basis.clone());
        }
    }
    output
}

/// Append the registered MZV basis as one contiguous tail block.
pub fn build_basis_atom_list(
    expansion: &MzvExpansionTable,
    user_indeterminates: impl IntoIterator<Item = Atom>,
) -> Vec<Atom> {
    let mut output = Vec::new();
    for atom in user_indeterminates
        .into_iter()
        .chain(expansion.basis_atoms.iter().cloned())
    {
        if !output.contains(&atom) {
            output.push(atom);
        }
    }
    output
}

/// Move a rational function between compatible named polynomial contexts.
///
/// Unlike the original FLINT port's pretty-string round trip, this remaps
/// sparse exponent vectors directly through Symbolica. The native integer
/// rational-polynomial representation remains the source of truth: context
/// rearrangement preserves coprimality, so only denominator-sign
/// normalization is required after the transfer.
pub fn cross_ctx_transfer_rat(source: &Rat, destination: Arc<PolyCtx>) -> Result<Rat> {
    if Arc::ptr_eq(source.ctx(), &destination) {
        return Ok(source.clone());
    }
    let numerator = source
        .native()
        .numerator
        .rearrange_with_growth(destination.native_variables())
        .map_err(Error::InvalidInput)?;
    let denominator = source
        .native()
        .denominator
        .rearrange_with_growth(destination.native_variables())
        .map_err(Error::InvalidInput)?;
    let native = NativeRat::from_num_den(numerator, denominator, &Z, false);
    Rat::from_native(destination, native)
}

/// Reject bridge payloads that contain wide-context reduction symbols.
pub fn assert_no_lhs_tokens(
    payload: &str,
    expansion: &MzvExpansionTable,
    site_name: &str,
) -> Result<()> {
    for token in tokens_in(payload) {
        if !looks_like_mzv(&token) {
            continue;
        }
        if expansion.basis_idx.contains_key(&token) {
            continue;
        }
        if expansion.expansion.contains_key(&token) {
            return Err(Error::InvalidInput(format!(
                "bridge input scanner ({site_name}): reducible MZV symbol `{token}` is not valid in a basis-only context"
            )));
        }
        return Err(Error::InvalidInput(format!(
            "bridge input scanner ({site_name}): unknown MZV-like token `{token}`"
        )));
    }
    Ok(())
}

fn canonical_basis(source: &[String]) -> Result<Vec<String>> {
    let mut output = Vec::with_capacity(source.len());
    let mut seen = HashSet::new();
    if source.iter().any(|name| name == "Log2") {
        output.push("Log2".to_owned());
        seen.insert("Log2".to_owned());
    }
    for name in source {
        if seen.insert(name.clone()) {
            output.push(name.clone());
        }
    }
    if output.is_empty() {
        return Err(Error::InvalidInput(
            "load_mzv_expansion: reduction table has an empty basis".into(),
        ));
    }
    Ok(output)
}

/// Load a production-flat table into a basis-only polynomial context.
pub fn load_mzv_expansion(path: impl AsRef<Path>) -> Result<MzvExpansionTable> {
    load_mzv_expansion_with_options(path, false)
}

/// Load and eagerly expand a table, optionally accepting weight-decreasing
/// chained rules (useful for generated-table validation fixtures).
pub fn load_mzv_expansion_with_options(
    path: impl AsRef<Path>,
    allow_chained: bool,
) -> Result<MzvExpansionTable> {
    let path = path.as_ref();
    let table = load_mzv_reductions(path)?;
    expand_mzv_reductions(&table, allow_chained, &path.display().to_string())
}

/// Eagerly expand a decoded reduction table into its basis-only context.
///
/// `source_name` is used only in validation diagnostics. This entry point is
/// what lets the embedded table avoid a temporary file or source-tree path.
pub fn expand_mzv_reductions(
    table: &MzvReductionTable,
    allow_chained: bool,
    source_name: &str,
) -> Result<MzvExpansionTable> {
    let mut reductions = table.reductions().to_vec();
    reductions.sort_by_key(|rule| weight_of_mzv_name(&rule.lhs));

    let basis_names = canonical_basis(table.basis())?;
    let basis_atoms = basis_names
        .iter()
        .map(|name| {
            mzv_constant_atom(name).ok_or_else(|| {
                Error::InvalidInput(format!(
                    "load_mzv_expansion: invalid basis identifier `{name}`"
                ))
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let basis_ctx = PolyCtx::from_indeterminates(basis_atoms.clone())?;
    let basis_idx = basis_names
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, name)| (name, index))
        .collect::<HashMap<_, _>>();
    let lhs_names = reductions
        .iter()
        .map(|rule| rule.lhs.clone())
        .collect::<HashSet<_>>();

    let mut any_chained = false;
    for rule in &reductions {
        let lhs_weight = weight_of_mzv_name(&rule.lhs);
        if lhs_weight < 0 {
            return Err(Error::InvalidInput(format!(
                "load_mzv_expansion: invalid rule LHS `{}`",
                rule.lhs
            )));
        }
        for token in tokens_in(&rule.rhs) {
            if basis_idx.contains_key(&token) {
                continue;
            }
            if lhs_names.contains(&token) {
                any_chained = true;
                let rhs_weight = weight_of_mzv_name(&token);
                if rhs_weight >= lhs_weight {
                    return Err(Error::InvalidInput(format!(
                        "load_mzv_expansion: `{}` (weight {lhs_weight}) references `{token}` (weight {rhs_weight}); strict decrease violated",
                        rule.lhs
                    )));
                }
            } else if looks_like_mzv(&token) {
                return Err(Error::InvalidInput(format!(
                    "load_mzv_expansion: `{}` references unknown MZV `{token}`",
                    rule.lhs
                )));
            }
        }
    }
    if any_chained && !allow_chained {
        return Err(Error::InvalidInput(format!(
            "load_mzv_expansion: {} contains chained rules but allow_chained is false",
            source_name
        )));
    }

    let mut output = MzvExpansionTable {
        basis_ctx,
        basis_atoms,
        basis_names,
        basis_idx,
        expansion: BTreeMap::new(),
    };
    for rule in &reductions {
        expand_rule(rule, &lhs_names, &mut output)?;
    }
    Ok(output)
}

static STANDARD_MZV_EXPANSION: OnceLock<std::result::Result<Arc<MzvExpansionTable>, String>> =
    OnceLock::new();

/// Return the cached basis-only expansion of the embedded standard table.
pub fn standard_mzv_expansion() -> Result<&'static MzvExpansionTable> {
    STANDARD_MZV_EXPANSION
        .get_or_init(|| {
            expand_mzv_reductions(
                &standard_mzv_reductions(),
                false,
                "embedded data/mzv_reductions.json",
            )
            .map(Arc::new)
            .map_err(|error| error.to_string())
        })
        .as_ref()
        .map(Arc::as_ref)
        .map_err(|error| {
            Error::InvalidInput(format!(
                "cannot initialize embedded standard MZV expansion: {error}"
            ))
        })
}

fn expand_rule(
    rule: &MzvReductionRule,
    lhs_names: &HashSet<String>,
    output: &mut MzvExpansionTable,
) -> Result<()> {
    if rule.rhs.trim() == "0" {
        output
            .expansion
            .insert(rule.lhs.clone(), Rat::zero(output.basis_ctx.clone()));
        return Ok(());
    }

    let references = tokens_in(&rule.rhs)
        .into_iter()
        .filter(|token| lhs_names.contains(token) && !output.basis_idx.contains_key(token))
        .collect::<Vec<_>>();
    if references.is_empty() {
        let atom = mzv_expression_atom(&rule.rhs)?;
        output.expansion.insert(
            rule.lhs.clone(),
            Rat::from_atom(output.basis_ctx.clone(), atom.as_view())?,
        );
        return Ok(());
    }

    let mut work_variables = output.basis_atoms.clone();
    work_variables.extend(
        references
            .iter()
            .map(|name| {
                mzv_constant_atom(name).ok_or_else(|| {
                    Error::InvalidInput(format!(
                        "load_mzv_expansion: invalid rule reference `{name}`"
                    ))
                })
            })
            .collect::<Result<Vec<_>>>()?,
    );
    let work_ctx = PolyCtx::from_indeterminates(work_variables)?;
    let rhs = mzv_expression_atom(&rule.rhs)?;
    let mut value = Rat::from_atom(work_ctx.clone(), rhs.as_view())?;
    for reference in references {
        let reference_atom = mzv_constant_atom(&reference).ok_or_else(|| {
            Error::InvalidInput(format!(
                "load_mzv_expansion: invalid rule reference `{reference}`"
            ))
        })?;
        let variable = work_ctx
            .index_of_indeterminate(reference_atom.as_view())
            .ok_or_else(|| {
                Error::InvalidInput(format!(
                    "load_mzv_expansion: internal context lost `{reference}`"
                ))
            })?;
        let prior = output.expansion.get(&reference).ok_or_else(|| {
            Error::InvalidInput(format!(
                "load_mzv_expansion: `{}` references `{reference}` before it was expanded",
                rule.lhs
            ))
        })?;
        let replacement = cross_ctx_transfer_rat(prior, work_ctx.clone())?;
        value = substitute_var_rat(&value, variable, &replacement)?;
    }
    let value = cross_ctx_transfer_rat(&value, output.basis_ctx.clone())?;
    output.expansion.insert(rule.lhs.clone(), value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use symbolica::prelude::{AtomCore, symbol};

    use crate::symbols::{heads, log_two_atom, mzv_atom};

    use super::*;

    #[test]
    fn name_weight_and_token_grammar_match_hyperflint() {
        assert_eq!(weight_of_mzv_name("Log2"), 1);
        assert_eq!(weight_of_mzv_name("mzv_m2_3"), 5);
        assert_eq!(weight_of_mzv_name("mzv_"), -1);
        assert_eq!(
            tokens_in("3*mzv_2 + Log2^2 - mzv_3"),
            ["mzv_2", "Log2", "mzv_3"]
        );
    }

    #[test]
    fn chained_rules_are_expanded_by_rational_substitution() {
        let fixture = std::env::temp_dir().join(format!(
            "hyperbolica-mzv-expansion-{}.json",
            std::process::id()
        ));
        fs::write(
            &fixture,
            r#"{
                "reductions": [
                    {"lhs":"mzv_4","rhs":"mzv_2*Log2^2-mzv_2^2"},
                    {"lhs":"mzv_8","rhs":"-mzv_4^2"}
                ],
                "basis": ["mzv_2", "Log2"]
            }"#,
        )
        .unwrap();
        let expansion = load_mzv_expansion_with_options(&fixture, true).unwrap();
        let expected_atom: Atom = -mzv_atom(&[2]).pow(2) * log_two_atom().pow(4)
            + 2 * mzv_atom(&[2]).pow(3) * log_two_atom().pow(2)
            - mzv_atom(&[2]).pow(4);
        let expected =
            Rat::from_atom(expansion.basis_ctx.clone(), expected_atom.as_view()).unwrap();
        assert_eq!(expansion.expansion["mzv_8"], expected);
        assert!(expansion.basis_atoms.iter().any(|atom| {
            atom.as_fun_view()
                .is_some_and(|f| f.get_symbol() == heads().mzv)
        }));
        assert!(!expansion.basis_atoms.iter().any(|atom| {
            atom.as_var_view()
                .is_some_and(|v| v.get_symbol().get_name().contains("mzv_"))
        }));
        let _ = fs::remove_file(fixture);
    }

    #[test]
    fn direct_context_transfer_respects_name_permutations() {
        let source_ctx = PolyCtx::new(["x", "y"]).unwrap();
        let destination_ctx = PolyCtx::new(["z", "y", "x"]).unwrap();
        let source = Rat::parse(source_ctx, "(x+y)/(1-x*y)").unwrap();
        let actual = cross_ctx_transfer_rat(&source, destination_ctx.clone()).unwrap();
        assert_eq!(
            actual,
            Rat::parse(destination_ctx, "(x+y)/(1-x*y)").unwrap()
        );
    }

    #[test]
    fn context_transfer_drops_only_unused_variables() {
        let source_ctx = PolyCtx::new(["x", "y"]).unwrap();
        let destination_ctx = PolyCtx::new(["x"]).unwrap();
        let independent = Rat::parse(source_ctx.clone(), "(x+1)/(x+2)").unwrap();
        assert_eq!(
            cross_ctx_transfer_rat(&independent, destination_ctx.clone()).unwrap(),
            Rat::parse(destination_ctx.clone(), "(x+1)/(x+2)").unwrap()
        );

        let dependent = Rat::parse(source_ctx, "(x+y)/(x-y)").unwrap();
        assert!(matches!(
            cross_ctx_transfer_rat(&dependent, destination_ctx),
            Err(Error::InvalidInput(_))
        ));
    }

    #[test]
    fn context_transfer_renormalizes_denominator_sign_after_permutation() {
        let source_ctx = PolyCtx::new(["x", "y"]).unwrap();
        let destination_ctx = PolyCtx::new(["y", "x"]).unwrap();
        let source = Rat::parse(source_ctx, "1/(x-y)").unwrap();
        let transferred = cross_ctx_transfer_rat(&source, destination_ctx.clone()).unwrap();
        assert_eq!(transferred, Rat::parse(destination_ctx, "1/(x-y)").unwrap());
    }

    #[test]
    fn context_transfer_preserves_function_indeterminates() {
        let f = symbol!("mzv_transfer_function");
        let call = f.call(11);
        let source_ctx = PolyCtx::from_indeterminates([call.clone(), mzv_atom(&[3])]).unwrap();
        let destination_ctx = PolyCtx::from_indeterminates([mzv_atom(&[3]), call.clone()]).unwrap();
        let source_atom = call + mzv_atom(&[3]);
        let source = Rat::from_atom(source_ctx, source_atom.as_view()).unwrap();
        let transferred = cross_ctx_transfer_rat(&source, destination_ctx.clone()).unwrap();
        assert_eq!(
            transferred,
            Rat::from_atom(destination_ctx, source_atom.as_view()).unwrap()
        );
    }
}
