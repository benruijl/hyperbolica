use pyo3::{PyResult, Python, pyclass, pymethods};
use symbolica::api::python::PythonExpression;

use crate::api::{AtomIntegrationOptions, PreparedAtomInput, integrate_prepared_atom};

use super::{exceptions, options::PythonIntegrationOptions, result::PythonIntegrationResult};

/// A lowered Atom input that can be integrated repeatedly without repeating
/// Symbolica-to-ring and Hlog conversion.
#[pyclass(
    frozen,
    skip_from_py_object,
    module = "hyperbolica",
    name = "PreparedIntegral"
)]
#[derive(Clone, Debug)]
pub struct PythonPreparedIntegral {
    inner: PreparedAtomInput,
    default_options: AtomIntegrationOptions,
}

impl PythonPreparedIntegral {
    pub(crate) fn new(inner: PreparedAtomInput, default_options: AtomIntegrationOptions) -> Self {
        Self {
            inner,
            default_options,
        }
    }

    fn run(
        &self,
        py: Python<'_>,
        options: Option<&PythonIntegrationOptions>,
    ) -> PyResult<PythonIntegrationResult> {
        let prepared = self.inner.clone();
        let options = options.map_or_else(
            || self.default_options.clone(),
            PythonIntegrationOptions::to_rust,
        );
        py.detach(move || integrate_prepared_atom(&prepared, &options))
            .map(PythonIntegrationResult::new)
            .map_err(exceptions::integration_error)
    }
}

#[pymethods]
impl PythonPreparedIntegral {
    /// Integration variables in their requested order.
    #[getter]
    fn integration_variables(&self) -> Vec<PythonExpression> {
        self.inner
            .integration_variables()
            .iter()
            .map(|symbol| PythonExpression::from(symbol.to_atom()))
            .collect()
    }

    /// Symbolica indeterminates in the prepared exact polynomial context.
    #[getter]
    fn indeterminates(&self) -> Vec<PythonExpression> {
        self.inner
            .indeterminates()
            .iter()
            .cloned()
            .map(PythonExpression::from)
            .collect()
    }

    /// Number of shuffle entries in the prepared integrand.
    #[getter]
    fn shuffle_term_count(&self) -> usize {
        self.inner.shuffle_list().len()
    }

    /// Integrate and return a native Symbolica expression.
    #[pyo3(signature = (options = None))]
    fn integrate(
        &self,
        py: Python<'_>,
        options: Option<&PythonIntegrationOptions>,
    ) -> PyResult<PythonExpression> {
        self.run(py, options)?.into_expression()
    }

    /// Integrate and retain metadata about the collected exact result.
    #[pyo3(signature = (options = None))]
    fn integrate_detailed(
        &self,
        py: Python<'_>,
        options: Option<&PythonIntegrationOptions>,
    ) -> PyResult<PythonIntegrationResult> {
        self.run(py, options)
    }

    fn __repr__(&self) -> String {
        format!(
            "PreparedIntegral(shuffle_term_count={}, variables={}, indeterminates={})",
            self.inner.shuffle_list().len(),
            self.inner.integration_variables().len(),
            self.inner.indeterminates().len()
        )
    }
}
