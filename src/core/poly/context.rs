use std::sync::Arc;

use symbolica::prelude::{Atom, AtomView, PolyVariable, Symbol};

use super::PolyCtx;
use crate::error::{Error, Result};
use crate::symbols::{SYMBOL_NAMESPACE, plain_atom_string};

impl PolyCtx {
    pub fn new<I, S>(variables: I) -> Result<Arc<Self>>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let names = variables.into_iter().map(Into::into).collect::<Vec<_>>();
        let mut seen = std::collections::HashSet::with_capacity(names.len());
        for name in &names {
            if name.is_empty() || !seen.insert(name.clone()) {
                return Err(Error::InvalidInput(format!(
                    "variable names must be non-empty and unique: `{name}`"
                )));
            }
        }

        let variables = names
            .iter()
            .map(|name| {
                Symbol::parse(name, SYMBOL_NAMESPACE)
                    .map(PolyVariable::Symbol)
                    .map_err(|message| Error::PolynomialParse {
                        expression: name.clone(),
                        message,
                    })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Arc::new(Self {
            names: Arc::new(names),
            variables: Arc::new(variables),
        }))
    }

    /// Build a polynomial context directly from Symbolica symbols.
    ///
    /// This is the primary construction path for the Atom-native API. String
    /// names are retained only for diagnostics and transport adapters.
    pub fn from_symbols<I>(symbols: I) -> Result<Arc<Self>>
    where
        I: IntoIterator<Item = Symbol>,
    {
        let symbols = symbols.into_iter().collect::<Vec<_>>();
        let mut seen = std::collections::HashSet::with_capacity(symbols.len());
        for symbol in &symbols {
            if !seen.insert(*symbol) {
                return Err(Error::InvalidInput(format!(
                    "polynomial symbols must be unique: `{}`",
                    symbol.get_name()
                )));
            }
        }
        let names = symbols
            .iter()
            .map(|symbol| symbol.get_name().to_owned())
            .collect::<Vec<_>>();
        let variables = symbols
            .into_iter()
            .map(PolyVariable::Symbol)
            .collect::<Vec<_>>();
        Ok(Arc::new(Self {
            names: Arc::new(names),
            variables: Arc::new(variables),
        }))
    }

    /// Build a context from arbitrary Symbolica polynomial indeterminates.
    ///
    /// Besides ordinary symbols, Symbolica can treat function calls and
    /// non-polynomial powers as atomic polynomial variables. This constructor
    /// preserves that native representation for Atom-based callers.
    pub fn from_indeterminates<I>(indeterminates: I) -> Result<Arc<Self>>
    where
        I: IntoIterator<Item = Atom>,
    {
        let indeterminates = indeterminates.into_iter().collect::<Vec<_>>();
        let names = indeterminates
            .iter()
            .map(plain_atom_string)
            .collect::<Vec<_>>();
        let variables = indeterminates
            .into_iter()
            .map(|atom| PolyVariable::try_from(atom).map_err(Error::InvalidInput))
            .collect::<Result<Vec<_>>>()?;
        let mut seen = std::collections::HashSet::with_capacity(variables.len());
        for variable in &variables {
            if !seen.insert(variable.clone()) {
                return Err(Error::InvalidInput(format!(
                    "polynomial indeterminates must be unique: `{variable}`"
                )));
            }
        }
        Ok(Arc::new(Self {
            names: Arc::new(names),
            variables: Arc::new(variables),
        }))
    }

    /// Build a context from native indeterminates while retaining explicit
    /// transport spellings for diagnostics and round trips.
    pub(crate) fn from_named_indeterminates<I, S>(
        names: I,
        indeterminates: impl IntoIterator<Item = Atom>,
    ) -> Result<Arc<Self>>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let names = names.into_iter().map(Into::into).collect::<Vec<_>>();
        let variables = indeterminates
            .into_iter()
            .map(|atom| PolyVariable::try_from(atom).map_err(Error::InvalidInput))
            .collect::<Result<Vec<_>>>()?;
        if names.len() != variables.len() {
            return Err(Error::InvalidInput(format!(
                "polynomial context has {} names but {} indeterminates",
                names.len(),
                variables.len()
            )));
        }
        let mut seen_names = std::collections::HashSet::with_capacity(names.len());
        for name in &names {
            if name.is_empty() || !seen_names.insert(name.clone()) {
                return Err(Error::InvalidInput(format!(
                    "variable names must be non-empty and unique: `{name}`"
                )));
            }
        }
        let mut seen_variables = std::collections::HashSet::with_capacity(variables.len());
        for variable in &variables {
            if !seen_variables.insert(variable.clone()) {
                return Err(Error::InvalidInput(format!(
                    "polynomial indeterminates must be unique: `{variable}`"
                )));
            }
        }
        Ok(Arc::new(Self {
            names: Arc::new(names),
            variables: Arc::new(variables),
        }))
    }

    /// Diagnostic/presentation spellings in context order.
    ///
    /// These strings are not mathematical identities: distinct namespaced
    /// variables may share one spelling, and one native variable can have a
    /// different qualified spelling depending on its construction path. Use
    /// [`Self::index_of_symbol`] or [`Self::index_of_indeterminate`] for
    /// semantic lookup.
    pub fn vars(&self) -> &[String] {
        self.names.as_slice()
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Find the first variable with this diagnostic spelling.
    ///
    /// This is a presentation/legacy-adapter helper and may be ambiguous.
    /// Mathematical code must use a structural lookup method instead.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|candidate| candidate == name)
    }

    pub fn index_of_symbol(&self, symbol: Symbol) -> Option<usize> {
        self.variables
            .iter()
            .position(|variable| variable == &symbol)
    }

    /// Locate an arbitrary Symbolica polynomial indeterminate structurally.
    pub fn index_of_indeterminate(&self, atom: AtomView<'_>) -> Option<usize> {
        let variable = PolyVariable::try_from(atom.to_owned()).ok()?;
        self.variables
            .iter()
            .position(|candidate| candidate == &variable)
    }

    pub fn variable_atom(&self, index: usize) -> Result<Atom> {
        self.variables
            .get(index)
            .cloned()
            .map(Atom::from)
            .ok_or_else(|| Error::UnknownVariable(index.to_string()))
    }

    pub(crate) fn variable_map(&self) -> Arc<Vec<PolyVariable>> {
        self.variables.clone()
    }

    /// Ordered native variable identity without cloning the shared map.
    pub(crate) fn native_variables(&self) -> &[PolyVariable] {
        self.variables.as_slice()
    }

    /// Compare a native polynomial's shared map without scanning every
    /// indeterminate when it was constructed in this context.
    pub(crate) fn has_variable_map(&self, variables: &Arc<Vec<PolyVariable>>) -> bool {
        Arc::ptr_eq(&self.variables, variables) || self.variables.as_ref() == variables.as_ref()
    }

    /// Return the context variables as Symbolica symbols.
    pub fn symbol_variables(&self) -> Result<Vec<Symbol>> {
        self.variables
            .iter()
            .map(|variable| match variable {
                PolyVariable::Symbol(symbol) => Ok(*symbol),
                _ => Err(Error::InvalidInput(
                    "polynomial context contains a non-symbol indeterminate".into(),
                )),
            })
            .collect()
    }

    /// Whether both contexts describe the same ordered Symbolica polynomial
    /// ring.
    ///
    /// The native [`PolyVariable`] sequence is the complete mathematical
    /// identity. Diagnostic names are deliberately excluded: Symbolica's
    /// display spelling strips namespaces for some indeterminates, and the
    /// same variable can acquire a qualified or stripped diagnostic name
    /// depending on which Atom-native constructor supplied it.
    pub fn is_compatible_with(&self, other: &Self) -> bool {
        self.has_variable_map(&other.variables)
    }
}
