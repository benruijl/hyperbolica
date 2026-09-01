//! Multiple-zeta-value reduction tables and exact substitutions.
//!
//! HyperFLINT stores its generated identities as strings in a JSON table.
//! This module keeps that transport format, but parses replacement
//! expressions into Symbolica-backed [`Rat`] values only when a rule is
//! actually used.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use symbolica::prelude::{Atom, AtomCore};

use crate::core::Rat;
use crate::error::{Error, Result};
use crate::symbols::{legacy, log_two_atom};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MzvReductionRule {
    pub lhs: String,
    pub rhs: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct MzvReductionData {
    #[serde(default)]
    reductions: Vec<MzvReductionRule>,
    #[serde(default)]
    basis: Vec<String>,
}

/// An immutable, cheaply cloned MZV reduction table.
///
/// Tables use shared storage because the embedded production table is large
/// and is part of every default integration configuration. Use
/// [`Self::from_parts`] to construct an explicit override and the setter
/// methods when mutating a cloned options object.
#[derive(Clone, Debug, Default)]
pub struct MzvReductionTable {
    data: Arc<MzvReductionData>,
}

impl PartialEq for MzvReductionTable {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data) || self.data.as_ref() == other.data.as_ref()
    }
}

impl Eq for MzvReductionTable {}

impl MzvReductionTable {
    pub fn from_parts(reductions: Vec<MzvReductionRule>, basis: Vec<String>) -> Self {
        Self::from_data(MzvReductionData { reductions, basis })
    }

    /// Canonicalize semantic copies of the embedded table once, at the
    /// construction boundary. Hot-path standard checks can then remain a
    /// constant-time shared-storage comparison.
    fn from_data(data: MzvReductionData) -> Self {
        if data.reductions.len() != EMBEDDED_MZV_REDUCTION_COUNT
            || data.basis.len() != EMBEDDED_MZV_BASIS_COUNT
        {
            return Self {
                data: Arc::new(data),
            };
        }
        let standard = standard_mzv_reductions();
        if data == *standard.data {
            standard
        } else {
            Self {
                data: Arc::new(data),
            }
        }
    }

    fn canonicalize_after_mutation(&mut self) {
        if self.data.reductions.len() != EMBEDDED_MZV_REDUCTION_COUNT
            || self.data.basis.len() != EMBEDDED_MZV_BASIS_COUNT
        {
            return;
        }
        let standard = standard_mzv_reductions();
        if self.data.as_ref() == standard.data.as_ref() {
            self.data = standard.data;
        }
    }

    pub fn reductions(&self) -> &[MzvReductionRule] {
        &self.data.reductions
    }

    pub fn basis(&self) -> &[String] {
        &self.data.basis
    }

    pub fn set_reductions(&mut self, reductions: Vec<MzvReductionRule>) {
        if self.data.reductions == reductions {
            return;
        }
        Arc::make_mut(&mut self.data).reductions = reductions;
        self.canonicalize_after_mutation();
    }

    pub fn set_basis(&mut self, basis: Vec<String>) {
        if self.data.basis == basis {
            return;
        }
        Arc::make_mut(&mut self.data).basis = basis;
        self.canonicalize_after_mutation();
    }

    pub fn is_empty(&self) -> bool {
        self.data.reductions.is_empty() && self.data.basis.is_empty()
    }

    pub(crate) fn is_embedded_standard(&self) -> bool {
        STANDARD_MZV_REDUCTIONS
            .get()
            .is_some_and(|standard| Arc::ptr_eq(&self.data, &standard.data))
    }
}

impl Serialize for MzvReductionTable {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.data.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MzvReductionTable {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        MzvReductionData::deserialize(deserializer).map(Self::from_data)
    }
}

const EMBEDDED_MZV_REDUCTIONS: &[u8] = include_bytes!("../../data/mzv_reductions.json");
// Cheap shape guards keep explicit empty and ordinary custom tables from
// initializing the large embedded table merely to prove they are different.
// The embedded-data test below keeps these generated-data counts synchronized.
const EMBEDDED_MZV_REDUCTION_COUNT: usize = 700;
const EMBEDDED_MZV_BASIS_COUNT: usize = 10;
static STANDARD_MZV_REDUCTIONS: OnceLock<MzvReductionTable> = OnceLock::new();

/// Return the generated standard table embedded in this crate.
///
/// JSON decoding happens once per process and clones share the decoded data.
/// The relative `include_bytes!` path makes native libraries and wheels
/// independent of the source checkout at runtime.
pub fn standard_mzv_reductions() -> MzvReductionTable {
    STANDARD_MZV_REDUCTIONS
        .get_or_init(|| {
            // Decode the backing data directly. Calling the table's
            // deserializer here would recursively ask this OnceLock for the
            // standard value while it is still being initialized.
            let data: MzvReductionData = serde_json::from_slice(EMBEDDED_MZV_REDUCTIONS)
                .expect("embedded MZV reduction data must be valid JSON");
            assert!(
                !data.reductions.is_empty(),
                "embedded MZV reduction data must contain rules"
            );
            MzvReductionTable {
                data: Arc::new(data),
            }
        })
        .clone()
}

/// Load a generated HyperFLINT MZV reduction table.
pub fn load_mzv_reductions(path: impl AsRef<Path>) -> Result<MzvReductionTable> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|error| {
        Error::InvalidInput(format!(
            "load_mzv_reductions: cannot open {}: {error}",
            path.display()
        ))
    })?;
    let table: MzvReductionTable = serde_json::from_reader(file)?;
    if table.reductions().is_empty() {
        return Err(Error::InvalidInput(format!(
            "load_mzv_reductions: no reductions in {}",
            path.display()
        )));
    }
    Ok(table)
}

/// Substitute one polynomial variable by an arbitrary rational function.
///
/// Numerator and denominator are expanded independently in the selected
/// variable, which avoids textual replacement and therefore preserves unary
/// minus, powers, and operator precedence exactly.
pub fn substitute_var_rat(rational: &Rat, variable: usize, replacement: &Rat) -> Result<Rat> {
    rational.substitute_rat(variable, replacement)
}

fn uses_variable(rational: &Rat, variable: usize) -> Result<bool> {
    rational.depends_on(variable)
}

/// Apply every reachable reduction rule until a fixed point is reached.
///
/// Rules whose left-hand side is absent from the polynomial context are a
/// no-op. Replacement expressions are parsed lazily, so a large production
/// table does not impose a parse cost for unrelated MZVs.
pub fn apply_mzv_reductions(table: &MzvReductionTable, rational: &Rat) -> Result<Rat> {
    let active = table
        .reductions()
        .iter()
        .filter_map(|rule| {
            mzv_constant_atom(&rule.lhs)
                .as_ref()
                .and_then(|atom| rational.ctx().index_of_indeterminate(atom.as_view()))
                .map(|index| (index, rule))
        })
        .collect::<Vec<_>>();
    if active.is_empty() {
        return Ok(rational.clone());
    }

    let mut parsed = HashMap::<usize, Rat>::new();
    let mut current = rational.clone();
    for _ in 0..50 {
        let mut changed = false;
        for (rule_index, &(variable, rule)) in active.iter().enumerate() {
            if !uses_variable(&current, variable)? {
                continue;
            }
            let replacement = if let Some(value) = parsed.get(&rule_index) {
                value.clone()
            } else {
                let atom = mzv_expression_atom(&rule.rhs)?;
                let value = Rat::from_atom(current.ctx().clone(), atom.as_view())?;
                parsed.insert(rule_index, value.clone());
                value
            };
            current = substitute_var_rat(&current, variable, &replacement)?;
            changed = true;
        }
        if !changed {
            return Ok(current);
        }
    }

    Err(Error::InvalidInput(
        "apply_mzv_reductions: substitution did not terminate in 50 passes".into(),
    ))
}

/// Union of user variables, `Log2`, basis names, and reducible MZV names.
pub fn build_mzv_var_list(
    table: &MzvReductionTable,
    user_variables: impl IntoIterator<Item = impl Into<String>>,
) -> Vec<String> {
    let mut output = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |name: String| {
        if seen.insert(name.clone()) {
            output.push(name);
        }
    };

    for name in user_variables {
        add(name.into());
    }
    add("Log2".into());
    for name in table.basis() {
        add(name.clone());
    }
    for rule in table.reductions() {
        add(rule.lhs.clone());
    }
    output
}

/// Decode HyperFLINT's legacy MZV identifier into signed indices.
///
/// This function belongs to the string compatibility boundary. Native code
/// represents the result with the single registered `MZV` function head.
pub fn indices_from_mzv_name(name: &str) -> Option<Vec<i64>> {
    let rest = name.strip_prefix("mzv_")?;
    if rest.is_empty() {
        return None;
    }
    rest.split('_')
        .map(|part| {
            let (negative, digits) = match part.strip_prefix('m') {
                Some(digits) => (true, digits),
                None => (false, part),
            };
            if digits.is_empty() {
                return None;
            }
            let value = digits.parse::<i64>().ok()?;
            Some(if negative { -value } else { value })
        })
        .collect()
}

/// Convert a legacy reduction-table identifier into a registered Atom.
pub fn mzv_constant_atom(name: &str) -> Option<Atom> {
    legacy::special_atom_from_name(name).filter(|atom| {
        atom == &log_two_atom()
            || atom
                .as_fun_view()
                .is_some_and(|function| function.get_symbol() == crate::symbols::heads().mzv)
    })
}

/// Union user indeterminates with all MZV constants as registered Atoms.
pub fn build_mzv_basis_atom_list(
    table: &MzvReductionTable,
    user_indeterminates: impl IntoIterator<Item = Atom>,
) -> Result<Vec<Atom>> {
    let mut output = Vec::new();
    let mut add = |atom: Atom| {
        if !output.contains(&atom) {
            output.push(atom);
        }
    };
    for atom in user_indeterminates {
        add(atom);
    }
    add(log_two_atom());
    for name in table.basis() {
        add(mzv_constant_atom(name).ok_or_else(|| {
            Error::InvalidInput(format!("invalid MZV reduction-table identifier `{name}`"))
        })?);
    }
    Ok(output)
}

/// Union user indeterminates with the basis and every reducible MZV.
///
/// This wide representation is retained for the HyperFLINT compatibility
/// bridge. The primary Atom API uses [`build_mzv_basis_atom_list`] with the
/// embedded standard table and expands generated constants into that basis.
pub fn build_mzv_atom_list(
    table: &MzvReductionTable,
    user_indeterminates: impl IntoIterator<Item = Atom>,
) -> Result<Vec<Atom>> {
    let mut output = Vec::new();
    let mut add = |atom: Atom| {
        if !output.contains(&atom) {
            output.push(atom);
        }
    };
    for atom in user_indeterminates {
        add(atom);
    }
    add(log_two_atom());
    for name in table
        .basis()
        .iter()
        .chain(table.reductions().iter().map(|rule| &rule.lhs))
    {
        add(mzv_constant_atom(name).ok_or_else(|| {
            Error::InvalidInput(format!("invalid MZV reduction-table identifier `{name}`"))
        })?);
    }
    Ok(output)
}

/// Parse a legacy reduction expression and replace every MZV identifier by a
/// registered function Atom in one structural pass.
pub fn mzv_expression_atom(expression: &str) -> Result<Atom> {
    legacy::parse_expression(expression)
}

/// C-style identifier tokens in source order, deduplicated on first use.
pub(crate) fn identifier_tokens(expression: &str) -> Vec<String> {
    let bytes = expression.as_bytes();
    let mut output = Vec::new();
    let mut seen = HashSet::new();
    let mut index = 0;
    while index < bytes.len() {
        let first = bytes[index];
        if first.is_ascii_alphabetic() || first == b'_' {
            let mut end = index + 1;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            let token = expression[index..end].to_owned();
            if seen.insert(token.clone()) {
                output.push(token);
            }
            index = end;
        } else {
            index += 1;
        }
    }
    output
}

/// A conservative per-input context that closes mentioned MZVs over the
/// reduction graph while always retaining the complete basis.
pub fn build_narrow_var_list(
    table: &MzvReductionTable,
    user_variables: &[String],
    expression: &str,
) -> Vec<String> {
    let full = build_mzv_var_list(table, user_variables.iter().cloned());
    let universe = full.iter().cloned().collect::<HashSet<_>>();
    let rules = table
        .reductions()
        .iter()
        .map(|rule| (rule.lhs.as_str(), rule))
        .collect::<HashMap<_, _>>();

    let mut reachable = user_variables.iter().cloned().collect::<HashSet<_>>();
    reachable.insert("Log2".into());
    reachable.extend(table.basis().iter().cloned());
    let mut work = user_variables
        .iter()
        .filter(|name| universe.contains(*name))
        .cloned()
        .collect::<Vec<_>>();
    for token in identifier_tokens(expression) {
        if universe.contains(&token) && !work.contains(&token) {
            work.push(token);
        }
    }
    reachable.extend(work.iter().cloned());
    while let Some(name) = work.pop() {
        let Some(rule) = rules.get(name.as_str()) else {
            continue;
        };
        for token in identifier_tokens(&rule.rhs) {
            if universe.contains(&token) && reachable.insert(token.clone()) {
                work.push(token);
            }
        }
    }

    full.into_iter()
        .filter(|name| reachable.contains(name))
        .collect()
}

#[cfg(test)]
mod tests {
    use symbolica::prelude::{AtomCore, symbol};

    use super::*;
    use crate::core::PolyCtx;
    use crate::symbols::mzv_atom;

    fn table() -> MzvReductionTable {
        MzvReductionTable::from_parts(
            vec![
                MzvReductionRule {
                    lhs: "mzv_4".into(),
                    rhs: "2/5*mzv_2^2".into(),
                },
                MzvReductionRule {
                    lhs: "mzv_6".into(),
                    rhs: "8/35*mzv_2^3".into(),
                },
            ],
            vec!["Log2".into(), "mzv_2".into(), "mzv_3".into()],
        )
    }

    #[test]
    fn rational_substitution_handles_denominators_and_precedence() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let rational = Rat::parse(ctx.clone(), "(x^2+y)/(1-x)").unwrap();
        let replacement = Rat::parse(ctx.clone(), "y/(1+y)").unwrap();
        let actual = substitute_var_rat(&rational, 0, &replacement).unwrap();
        let expected = Rat::parse(ctx, "((y/(1+y))^2+y)/(1-y/(1+y))").unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn fixed_point_reduction_is_exact_and_skips_absent_rules() {
        let z = symbol!("fixed_point_mzv_z");
        let ctx =
            PolyCtx::from_indeterminates(build_mzv_atom_list(&table(), [z.to_atom()]).unwrap())
                .unwrap();
        let input_atom: Atom = 3 * mzv_atom(&[4]) / (1 + z);
        let input = Rat::from_atom(ctx.clone(), input_atom.as_view()).unwrap();
        let actual = apply_mzv_reductions(&table(), &input).unwrap();
        let expected: Atom = (6 * mzv_atom(&[2]).pow(2)) / (5 * (1 + z));
        assert_eq!(actual, Rat::from_atom(ctx, expected.as_view()).unwrap());
        assert!(!input.compatibility_views_initialized());
        assert!(!actual.compatibility_views_initialized());
    }

    #[test]
    fn variable_list_has_stable_source_order() {
        assert_eq!(
            build_mzv_var_list(&table(), ["x", "Log2"]),
            ["x", "Log2", "mzv_2", "mzv_3", "mzv_4", "mzv_6"]
        );
    }

    #[test]
    fn native_mzv_atoms_use_registered_parameterized_heads() {
        assert_eq!(indices_from_mzv_name("mzv_m2_3"), Some(vec![-2, 3]));
        assert_eq!(indices_from_mzv_name("mzv_"), None);

        let atoms = build_mzv_atom_list(&table(), Vec::new()).unwrap();
        assert!(atoms.contains(&mzv_atom(&[2])));
        assert!(atoms.contains(&mzv_atom(&[4])));
        assert!(atoms.contains(&log_two_atom()));
        assert!(!atoms.iter().any(|atom| {
            atom.as_var_view()
                .is_some_and(|variable| variable.get_symbol().get_name().contains("mzv_"))
        }));

        let expression = mzv_expression_atom("2/5*mzv_2^2+Log2*mzv_m2_3").unwrap();
        assert!(expression.contains_symbol(crate::symbols::heads().mzv));
        assert!(expression.contains_symbol(crate::symbols::heads().log_two));
    }

    #[test]
    fn embedded_standard_table_is_shared_and_basis_only_is_narrow() {
        let first = standard_mzv_reductions();
        let second = standard_mzv_reductions();
        assert!(first.is_embedded_standard());
        assert!(second.is_embedded_standard());
        assert_eq!(first.reductions().len(), EMBEDDED_MZV_REDUCTION_COUNT);
        assert_eq!(first.basis().len(), EMBEDDED_MZV_BASIS_COUNT);
        assert!(first.reductions().len() > first.basis().len());

        let basis = build_mzv_basis_atom_list(&first, Vec::new()).unwrap();
        let wide = build_mzv_atom_list(&first, Vec::new()).unwrap();
        assert_eq!(basis.len(), first.basis().len());
        assert!(wide.len() > basis.len());

        let user = vec!["x".to_owned()];
        let no_mzv = build_narrow_var_list(&first, &user, "1/(1+x)");
        assert_eq!(no_mzv.len(), first.basis().len() + 1);
        assert!(no_mzv.contains(&"x".to_owned()));
        assert!(!no_mzv.contains(&first.reductions()[0].lhs));
    }

    #[test]
    fn semantic_standard_copies_recover_constant_time_identity() {
        let standard = standard_mzv_reductions();
        let from_parts = MzvReductionTable::from_parts(
            standard.reductions().to_vec(),
            standard.basis().to_vec(),
        );
        assert!(from_parts.is_embedded_standard());

        let encoded = serde_json::to_vec(&standard).unwrap();
        let decoded: MzvReductionTable = serde_json::from_slice(&encoded).unwrap();
        assert!(decoded.is_embedded_standard());

        let mut through_setters = MzvReductionTable::default();
        through_setters.set_reductions(standard.reductions().to_vec());
        assert!(!through_setters.is_embedded_standard());
        through_setters.set_basis(standard.basis().to_vec());
        assert!(through_setters.is_embedded_standard());

        // No-op setters retain the canonical allocation, without Arc cloning.
        through_setters.set_reductions(standard.reductions().to_vec());
        through_setters.set_basis(standard.basis().to_vec());
        assert!(through_setters.is_embedded_standard());
    }

    #[test]
    fn reductions_operate_on_registered_function_indeterminates() {
        let z = symbol!("native_mzv_z");
        let indeterminates = build_mzv_atom_list(&table(), [z.to_atom()]).unwrap();
        let ctx = PolyCtx::from_indeterminates(indeterminates).unwrap();
        let input_atom: Atom = 3 * mzv_atom(&[4]) / (1 + z);
        let input = Rat::from_atom(ctx.clone(), input_atom.as_view()).unwrap();
        let actual = apply_mzv_reductions(&table(), &input).unwrap();
        let expected_atom: Atom = (6 * mzv_atom(&[2]).pow(2)) / (5 * (1 + z));
        let expected = Rat::from_atom(ctx, expected_atom.as_view()).unwrap();
        assert_eq!(actual, expected);
    }
}
