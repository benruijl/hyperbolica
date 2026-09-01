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
    Bound, Py, PyResult, Python,
    exceptions::PyValueError,
    pyclass, pyfunction, pymethods, pymodule,
    types::{PyAny, PyAnyMethods, PyBytes, PyDictMethods, PyModule, PyModuleMethods},
    wrap_pyfunction,
};
use symbolica::{
    LicenseManager,
    api::python::{PythonExpression, create_symbolica_module},
    prelude::{Atom, AtomCore},
};

pub use options::PythonIntegrationOptions;
pub use prepared::PythonPreparedIntegral;
pub use result::{PythonAlgebraicLetter, PythonIntegrationResult};

/// Exact-type `copyreg` reducer for the Symbolica class embedded in the
/// standalone wheel.
///
/// Symbolica's ordinary Python distribution reconstructs expressions through
/// its own `symbolica` module. A Hyperbolica wheel embeds the same Rust class
/// in a differently named extension, so importing that hard-coded module can
/// either fail or return an object owned by a second native kernel. Capturing
/// the package-local reconstructor avoids both outcomes without installing a
/// `sys.modules` alias. The reducer is registered only by
/// [`register_standalone`], never by the community-module adapter.
#[pyclass(frozen, skip_from_py_object)]
struct StandaloneExpressionReducer {
    reconstruct: Py<PyAny>,
}

#[pymethods]
impl StandaloneExpressionReducer {
    fn __call__<'py>(
        &self,
        expression: &PythonExpression,
        py: Python<'py>,
    ) -> PyResult<(Py<PyAny>, (Py<PyBytes>,))> {
        let mut state = Vec::new();
        expression.expr.export(&mut state).map_err(|error| {
            PyValueError::new_err(format!("could not export Symbolica expression: {error}"))
        })?;
        let state = PyBytes::new(py, &state).unbind();
        Ok((self.reconstruct.clone_ref(py), (state,)))
    }
}

/// Reconstruct an expression from Symbolica's portable atom-and-state format.
///
/// The vendored Python helper accepts raw atom storage, whose symbol IDs are
/// process-local. Pickles must instead merge the exported partial symbol state
/// before decoding the atom so they remain valid in a fresh interpreter.
#[pyfunction]
fn _reconstruct_portable_expression(state: Vec<u8>) -> PyResult<PythonExpression> {
    let mut source = std::io::Cursor::new(state.as_slice());
    let expression = Atom::import(&mut source, None).map_err(|error| {
        PyValueError::new_err(format!("could not import Symbolica expression: {error}"))
    })?;
    if source.position() != state.len() as u64 {
        return Err(PyValueError::new_err(
            "Symbolica expression pickle contains trailing bytes",
        ));
    }
    Ok(expression.into())
}

fn register_standalone_expression_reducer(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = module.py();
    let reducer = Py::new(
        py,
        StandaloneExpressionReducer {
            reconstruct: module.getattr("_reconstruct_portable_expression")?.unbind(),
        },
    )?;
    py.import("copyreg")?
        .getattr("pickle")?
        .call1((module.getattr("Expression")?, reducer))?;
    Ok(())
}

/// Register the public Python API in an existing module.
///
/// Keeping registration separate from the `#[pymodule]` entry point also lets
/// a combined Symbolica distribution embed Hyperbolica without duplicating
/// any mathematical wrapper code.
fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    exceptions::register(module)?;
    module.add_class::<PythonIntegrationOptions>()?;
    module.add_class::<PythonPreparedIntegral>()?;
    module.add_class::<PythonAlgebraicLetter>()?;
    module.add_class::<PythonIntegrationResult>()?;
    functions::register(module)?;
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add("__symbolica_version__", LicenseManager::get_version())?;
    module.add("__api_version__", 1_u32)?;
    module.add(
        "__license__",
        "Mixed: Hyperbolica code is MIT; bundled Symbolica is separately licensed",
    )?;
    module.add(
        "__symbolica_license__",
        "Redistribution requires express prior permission; see DISTRIBUTION-LICENSE.md",
    )?;

    // Maturin's pure-Rust wheel wrapper uses `from .hyperbolica import *`.
    // Supplying a complete list makes that wrapper deterministic and carries
    // version metadata (normally excluded from star imports) to the package.
    let mut public_names = Vec::<String>::new();
    for (name, _) in module.dict().iter() {
        let name = name.extract::<String>()?;
        if !name.starts_with('_')
            || matches!(
                name.as_str(),
                "__api_version__"
                    | "__license__"
                    | "__symbolica_license__"
                    | "__symbolica_version__"
                    | "__version__"
            )
        {
            public_names.push(name);
        }
    }
    public_names.push("__all__".to_owned());
    public_names.sort_unstable();
    public_names.dedup();
    module.add("__all__", public_names)?;
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
    module.add_function(wrap_pyfunction!(_reconstruct_portable_expression, module)?)?;
    register_standalone_expression_reducer(module)?;
    register(module)
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
        types::{PyAnyMethods, PyDict, PyDictMethods, PyList, PyModule},
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

            let exports = module.getattr("__all__")?.extract::<Vec<String>>()?;
            for required in [
                "Expression",
                "IntegrationOptions",
                "__all__",
                "__api_version__",
                "__license__",
                "__symbolica_license__",
                "__symbolica_version__",
                "__version__",
                "integrate",
                "integrate_detailed_over",
                "integrate_over",
                "prepare",
            ] {
                assert!(exports.iter().any(|name| name == required));
            }
            assert!(exports.windows(2).all(|names| names[0] < names[1]));
            assert_eq!(
                module
                    .getattr("__symbolica_version__")?
                    .extract::<String>()?,
                LicenseManager::get_version()
            );

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

            // The standalone reducer must never fall through to Symbolica's
            // distribution-specific module import. A sentinel makes that
            // failure mode observable without loading a second native kernel.
            let modules = py
                .import("sys")?
                .getattr("modules")?
                .cast_into::<PyDict>()?;
            let previous_symbolica = modules.get_item("symbolica")?;
            let sentinel = py
                .import("types")?
                .getattr("ModuleType")?
                .call1(("symbolica",))?;
            modules.set_item("symbolica", &sentinel)?;
            let deep = py
                .import("copy")?
                .getattr("deepcopy")?
                .call1((&expression,))?;
            assert!(deep.is_instance(&expression_type)?);
            assert_eq!(
                deep.call_method0("to_canonical_string")?
                    .extract::<String>()?,
                expression
                    .call_method0("to_canonical_string")?
                    .extract::<String>()?
            );
            assert!(modules.get_item("symbolica")?.unwrap().is(&sentinel));
            if let Some(previous) = previous_symbolica {
                modules.set_item("symbolica", previous)?;
            } else {
                modules.del_item("symbolica")?;
            }
            Ok::<_, pyo3::PyErr>(())
        })
        .unwrap();
    }
}
