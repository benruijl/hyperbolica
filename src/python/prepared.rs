use std::sync::Arc;

use pyo3::{PyResult, Python, pyclass, pymethods};
use symbolica::api::python::PythonExpression;

use crate::api::{AtomIntegrationOptions, PreparedAtomInput, integrate_prepared_atom};

use super::{exceptions, options::PythonIntegrationOptions, result::PythonIntegrationResult};

/// A lowered Atom input that can be integrated repeatedly without repeating
/// Symbolica-to-ring and Hlog conversion.
///
/// Create instances with `integration.prepare`. The object is
/// immutable; `copy.copy` and `copy.deepcopy` return independent Python
/// handles that share its immutable backing storage.
#[cfg_attr(feature = "python_stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(
    frozen,
    skip_from_py_object,
    module = "symbolica.community.hepkit.integration",
    name = "PreparedIntegral"
)]
#[derive(Clone, Debug)]
pub struct PythonPreparedIntegral {
    inner: Arc<PreparedAtomInput>,
    default_options: AtomIntegrationOptions,
}

impl PythonPreparedIntegral {
    pub(crate) fn new(inner: PreparedAtomInput, default_options: AtomIntegrationOptions) -> Self {
        Self {
            inner: Arc::new(inner),
            default_options,
        }
    }

    fn run(
        &self,
        py: Python<'_>,
        options: Option<&PythonIntegrationOptions>,
    ) -> PyResult<PythonIntegrationResult> {
        super::record_usage();
        let prepared = Arc::clone(&self.inner);
        let options = options.map_or_else(
            || self.default_options.clone(),
            PythonIntegrationOptions::to_rust,
        );
        py.detach(move || integrate_prepared_atom(&prepared, &options))
            .map(PythonIntegrationResult::new)
            .map_err(|error| exceptions::integration_error(py, error))
    }
}

#[cfg_attr(feature = "python_stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
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

    /// Number of requested integration variables.
    #[getter]
    fn variable_count(&self) -> usize {
        self.inner.integration_variables().len()
    }

    /// Number of exact Symbolica indeterminates in the prepared context.
    #[getter]
    fn indeterminate_count(&self) -> usize {
        self.inner.indeterminates().len()
    }

    /// Independent copy of the options captured during preparation.
    #[getter]
    fn options(&self) -> PythonIntegrationOptions {
        PythonIntegrationOptions::from_rust(self.default_options.clone())
    }

    /// Integrate and return a native Symbolica expression.
    #[pyo3(
        signature = (options = None),
        text_signature = "(self, options=None)"
    )]
    fn integrate(
        &self,
        py: Python<'_>,
        options: Option<&PythonIntegrationOptions>,
    ) -> PyResult<PythonExpression> {
        self.run(py, options)?.into_expression(py)
    }

    /// Integrate and retain metadata about the collected exact result.
    #[pyo3(
        signature = (options = None),
        text_signature = "(self, options=None)"
    )]
    fn integrate_detailed(
        &self,
        py: Python<'_>,
        options: Option<&PythonIntegrationOptions>,
    ) -> PyResult<PythonIntegrationResult> {
        self.run(py, options)
    }

    /// Return the number of prepared shuffle entries.
    fn __len__(&self) -> usize {
        self.inner.shuffle_list().len()
    }

    fn __copy__(&self) -> Self {
        self.clone()
    }

    fn __deepcopy__(&self, _memo: &pyo3::Bound<'_, pyo3::PyAny>) -> Self {
        self.clone()
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
