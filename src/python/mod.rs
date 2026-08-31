//! Optional PyO3 bindings for the Atom-native public API.
//!
//! This module is compiled only with the `python` Cargo feature.  Expressions
//! cross the Python boundary as Symbolica's native `PythonExpression`; the
//! binding never formats and reparses mathematical input.

mod exceptions;
mod functions;
mod input;
mod options;
mod prepared;
mod result;

use pyo3::{
    Bound, PyResult, pymodule,
    types::{PyModule, PyModuleMethods},
};
use symbolica::api::python::{SymbolicaCommunityModule, create_symbolica_module};

pub use options::PythonIntegrationOptions;
pub use prepared::PythonPreparedIntegral;
pub use result::{PythonAlgebraicLetter, PythonIntegrationResult};

/// Register the public Python API in an existing module.
///
/// Keeping registration separate from the `#[pymodule]` entry point also lets
/// a combined Symbolica distribution embed Hyperbolica without duplicating
/// any mathematical wrapper code.
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    exceptions::register(module)?;
    module.add_class::<PythonIntegrationOptions>()?;
    module.add_class::<PythonPreparedIntegral>()?;
    module.add_class::<PythonAlgebraicLetter>()?;
    module.add_class::<PythonIntegrationResult>()?;
    functions::register(module)?;
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

/// Register a coherent standalone module containing the vendored Symbolica
/// expression type and Hyperbolica's algorithms in the same extension.
///
/// This avoids relying on PyO3 class identity across separately compiled
/// wheels: `hyperbolica.E(...)`, `hyperbolica.S(...)`, `prepare`, and
/// `integrate` all use this one kernel's `PythonExpression` type object.
pub fn register_standalone(module: &Bound<'_, PyModule>) -> PyResult<()> {
    create_symbolica_module(module)?;
    register(module)
}

/// Adapter for distributions that bundle Hyperbolica as a Symbolica
/// community module. This is the preferred packaging route when an existing
/// `symbolica` Python installation must share its exact Rust kernel and Python
/// expression type with Hyperbolica.
pub struct CommunityModule;

impl SymbolicaCommunityModule for CommunityModule {
    fn get_name() -> String {
        "hyperbolica".to_owned()
    }

    fn register_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
        register(module)
    }
}

/// Exact hyperlogarithm integration backed entirely by Symbolica.
#[pymodule(gil_used = true)]
pub fn hyperbolica(module: &Bound<'_, PyModule>) -> PyResult<()> {
    register_standalone(module)
}

#[cfg(test)]
mod tests {
    use pyo3::{
        Python,
        types::{PyAnyMethods, PyList, PyModule},
    };

    use super::*;

    #[test]
    fn standalone_module_constructs_the_expression_type_accepted_by_prepare() {
        Python::initialize();
        Python::attach(|py| {
            let module = PyModule::new(py, "hyperbolica")?;
            register_standalone(&module)?;

            let expression = module.getattr("E")?.call1(("python_smoke_x+1",))?;
            let expression_type = module.getattr("Expression")?;
            assert!(expression.is_instance(&expression_type)?);

            // Use a non-symbol integration variable so preparation stops at
            // Hyperbolica's structural input validator. Reaching its typed
            // InputError proves the exact class constructed above crossed the
            // shipped module boundary without formatting or reparsing.
            let variables = PyList::new(py, [&expression])?;
            let error = module
                .getattr("prepare")?
                .call1((&expression, variables))
                .unwrap_err();
            assert!(error.is_instance_of::<crate::python::exceptions::InputError>(py));
            assert!(
                error
                    .to_string()
                    .contains("must be a plain Symbolica symbol")
            );
            Ok::<_, pyo3::PyErr>(())
        })
        .unwrap();
    }
}
