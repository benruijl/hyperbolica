use pyo3::{PyResult, pyclass, pymethods};
use symbolica::api::python::PythonExpression;

use crate::{algebra::AlgebraicLetterEntry, api::AtomIntegrationOutput};

use super::exceptions;

/// Metadata for one formal quadratic-root pair allocated by an integration.
#[pyclass(
    frozen,
    skip_from_py_object,
    module = "hyperbolica",
    name = "AlgebraicLetter"
)]
#[derive(Clone, Debug)]
pub struct PythonAlgebraicLetter {
    inner: AlgebraicLetterEntry,
}

impl From<&AlgebraicLetterEntry> for PythonAlgebraicLetter {
    fn from(entry: &AlgebraicLetterEntry) -> Self {
        Self {
            inner: entry.clone(),
        }
    }
}

#[pymethods]
impl PythonAlgebraicLetter {
    #[getter]
    fn index(&self) -> usize {
        self.inner.idx
    }

    #[getter]
    fn integration_variable_index(&self) -> usize {
        self.inner.var_idx
    }

    #[getter]
    fn polynomial(&self) -> PythonExpression {
        PythonExpression::from(self.inner.polynomial.to_atom())
    }

    #[getter]
    fn leading_coefficient(&self) -> PythonExpression {
        PythonExpression::from(self.inner.lc.to_atom())
    }

    #[getter]
    fn root_sum(&self) -> PythonExpression {
        PythonExpression::from(self.inner.sum_value.to_atom())
    }

    #[getter]
    fn root_product(&self) -> PythonExpression {
        PythonExpression::from(self.inner.product_value.to_atom())
    }

    #[getter]
    fn discriminant(&self) -> PythonExpression {
        PythonExpression::from(self.inner.discriminant.to_atom())
    }

    #[getter]
    fn minus(&self) -> PythonExpression {
        PythonExpression::from(self.inner.atoms().minus)
    }

    #[getter]
    fn plus(&self) -> PythonExpression {
        PythonExpression::from(self.inner.atoms().plus)
    }

    fn __repr__(&self) -> String {
        format!(
            "AlgebraicLetter(index={}, variable_index={})",
            self.inner.idx, self.inner.var_idx
        )
    }
}

/// An inspectable exact integration result.
///
/// Use `expression` to materialize the collected terms as a native Symbolica
/// expression.  The direct `integrate` function performs this step for you.
#[pyclass(
    frozen,
    skip_from_py_object,
    module = "hyperbolica",
    name = "IntegrationResult"
)]
#[derive(Clone, Debug)]
pub struct PythonIntegrationResult {
    inner: AtomIntegrationOutput,
}

impl PythonIntegrationResult {
    pub(crate) fn new(inner: AtomIntegrationOutput) -> Self {
        Self { inner }
    }

    pub(crate) fn into_expression(self) -> PyResult<PythonExpression> {
        self.inner
            .to_atom()
            .map(PythonExpression::from)
            .map_err(exceptions::integration_error)
    }
}

#[pymethods]
impl PythonIntegrationResult {
    /// Materialize all collected terms as one normalized Symbolica expression.
    #[getter]
    fn expression(&self) -> PyResult<PythonExpression> {
        self.inner
            .to_atom()
            .map(PythonExpression::from)
            .map_err(exceptions::integration_error)
    }

    /// Integration variables in the order in which they were integrated.
    #[getter]
    fn integration_variables(&self) -> Vec<PythonExpression> {
        self.inner
            .integration_variables()
            .iter()
            .map(|symbol| PythonExpression::from(symbol.to_atom()))
            .collect()
    }

    /// Symbolica indeterminates in the exact polynomial context.
    #[getter]
    fn indeterminates(&self) -> Vec<PythonExpression> {
        self.inner
            .indeterminates()
            .iter()
            .cloned()
            .map(PythonExpression::from)
            .collect()
    }

    /// Formal quadratic-root definitions introduced by the integration.
    #[getter]
    fn algebraic_letters(&self) -> Vec<PythonAlgebraicLetter> {
        self.inner
            .algebraic_letters()
            .iter()
            .map(PythonAlgebraicLetter::from)
            .collect()
    }

    /// Number of collected coefficient/period-product terms.
    #[getter]
    fn term_count(&self) -> usize {
        self.inner.terms().len()
    }

    #[getter]
    fn algebraic_letter_count(&self) -> usize {
        self.inner.algebraic_letters().len()
    }

    /// Whether the exact result is zero.
    #[getter]
    fn is_zero(&self) -> bool {
        self.inner.is_zero()
    }

    fn __repr__(&self) -> String {
        format!(
            "IntegrationResult(term_count={}, variables={})",
            self.inner.terms().len(),
            self.inner.integration_variables().len()
        )
    }
}
