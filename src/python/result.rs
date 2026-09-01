use std::sync::Arc;

use pyo3::{PyResult, Python, pyclass, pymethods};
use symbolica::api::python::PythonExpression;

use crate::{algebra::AlgebraicLetterEntry, api::AtomIntegrationOutput};

use super::exceptions;

/// Immutable metadata for one formal quadratic-root pair allocated by an
/// integration.
#[pyclass(
    frozen,
    skip_from_py_object,
    module = "hyperbolica",
    name = "AlgebraicLetter"
)]
#[derive(Clone, Debug)]
pub struct PythonAlgebraicLetter {
    inner: Arc<AlgebraicLetterEntry>,
}

impl From<&AlgebraicLetterEntry> for PythonAlgebraicLetter {
    fn from(entry: &AlgebraicLetterEntry) -> Self {
        Self {
            inner: Arc::new(entry.clone()),
        }
    }
}

#[pymethods]
impl PythonAlgebraicLetter {
    /// Stable one-based index of this formal root pair.
    #[getter]
    fn index(&self) -> usize {
        self.inner.idx
    }

    /// Zero-based exact-context index of the integration variable that
    /// introduced this root pair.
    #[getter]
    fn integration_variable_index(&self) -> usize {
        self.inner.var_idx
    }

    /// Defining polynomial for the quadratic root pair.
    #[getter]
    fn polynomial(&self) -> PythonExpression {
        PythonExpression::from(self.inner.polynomial.to_atom())
    }

    /// Leading coefficient of the defining polynomial.
    #[getter]
    fn leading_coefficient(&self) -> PythonExpression {
        PythonExpression::from(self.inner.lc.to_atom())
    }

    /// Sum of the two formal roots.
    #[getter]
    fn root_sum(&self) -> PythonExpression {
        PythonExpression::from(self.inner.sum_value.to_atom())
    }

    /// Product of the two formal roots.
    #[getter]
    fn root_product(&self) -> PythonExpression {
        PythonExpression::from(self.inner.product_value.to_atom())
    }

    /// Discriminant of the defining polynomial.
    #[getter]
    fn discriminant(&self) -> PythonExpression {
        PythonExpression::from(self.inner.discriminant.to_atom())
    }

    /// Native Symbolica expression for the minus root.
    #[getter]
    fn minus(&self) -> PythonExpression {
        PythonExpression::from(self.inner.atoms().minus)
    }

    /// Native Symbolica expression for the plus root.
    #[getter]
    fn plus(&self) -> PythonExpression {
        PythonExpression::from(self.inner.atoms().plus)
    }

    fn __copy__(&self) -> Self {
        self.clone()
    }

    fn __deepcopy__(&self, _memo: &pyo3::Bound<'_, pyo3::PyAny>) -> Self {
        self.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "AlgebraicLetter(index={}, integration_variable_index={})",
            self.inner.idx, self.inner.var_idx
        )
    }
}

/// An inspectable exact integration result.
///
/// Use `expression` to materialize the collected terms as a native Symbolica
/// expression.  The direct `integrate` function performs this step for you.
/// Instances are immutable and support `copy.copy` and `copy.deepcopy`.
#[pyclass(
    frozen,
    skip_from_py_object,
    module = "hyperbolica",
    name = "IntegrationResult"
)]
#[derive(Clone, Debug)]
pub struct PythonIntegrationResult {
    inner: Arc<AtomIntegrationOutput>,
}

impl PythonIntegrationResult {
    pub(crate) fn new(inner: AtomIntegrationOutput) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    pub(crate) fn into_expression(self, py: Python<'_>) -> PyResult<PythonExpression> {
        self.inner
            .to_atom()
            .map(PythonExpression::from)
            .map_err(|error| exceptions::integration_error(py, error))
    }
}

#[pymethods]
impl PythonIntegrationResult {
    /// Materialize all collected terms as one normalized Symbolica expression.
    #[getter]
    fn expression(&self, py: Python<'_>) -> PyResult<PythonExpression> {
        self.inner
            .to_atom()
            .map(PythonExpression::from)
            .map_err(|error| exceptions::integration_error(py, error))
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

    /// Number of formal quadratic-root pairs introduced by integration.
    #[getter]
    fn algebraic_letter_count(&self) -> usize {
        self.inner.algebraic_letters().len()
    }

    /// Number of integration variables in the completed schedule.
    #[getter]
    fn variable_count(&self) -> usize {
        self.inner.integration_variables().len()
    }

    /// Number of exact Symbolica indeterminates in the result context.
    #[getter]
    fn indeterminate_count(&self) -> usize {
        self.inner.indeterminates().len()
    }

    /// Whether the exact result is zero.
    #[getter]
    fn is_zero(&self) -> bool {
        self.inner.is_zero()
    }

    /// Return the number of collected coefficient/period-product terms.
    fn __len__(&self) -> usize {
        self.inner.terms().len()
    }

    fn __copy__(&self) -> Self {
        self.clone()
    }

    fn __deepcopy__(&self, _memo: &pyo3::Bound<'_, pyo3::PyAny>) -> Self {
        self.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "IntegrationResult(term_count={}, variables={}, indeterminates={}, \
             algebraic_letters={}, is_zero={})",
            self.inner.terms().len(),
            self.inner.integration_variables().len(),
            self.inner.indeterminates().len(),
            self.inner.algebraic_letters().len(),
            if self.inner.is_zero() {
                "True"
            } else {
                "False"
            },
        )
    }
}
