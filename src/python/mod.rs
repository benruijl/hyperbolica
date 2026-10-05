//! HEPkit bindings sharing the host's Symbolica kernel.
mod exceptions;
mod functions;
mod input;
mod options;
mod prepared;
mod result;

use pyo3::{
    Bound, PyResult, Python,
    types::{PyAnyMethods, PyDictMethods, PyListMethods, PyModule, PyModuleMethods},
};
use std::sync::atomic::{AtomicBool, Ordering};
use symbolica::api::python::{Citation, SymbolicaCommunityModule};
use symbolica::license::LicenseManager;

pub use options::PythonIntegrationOptions;
pub use prepared::PythonPreparedIntegral;
pub use result::{PythonAlgebraicLetter, PythonIntegrationResult};

static USED: AtomicBool = AtomicBool::new(false);
pub(crate) fn record_usage() {
    USED.store(true, Ordering::Relaxed);
}

/// Register exact definite integration in the host's existing Symbolica kernel.
pub struct CommunityModule;
impl SymbolicaCommunityModule for CommunityModule {
    fn get_name() -> String {
        "hepkit_integration".into()
    }

    fn register_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
        exceptions::register(module)?;
        module.add_class::<PythonIntegrationOptions>()?;
        module.add_class::<PythonPreparedIntegral>()?;
        module.add_class::<PythonAlgebraicLetter>()?;
        module.add_class::<PythonIntegrationResult>()?;
        functions::register(module)?;
        module.add("__version__", env!("CARGO_PKG_VERSION"))?;
        module.add("__symbolica_version__", LicenseManager::get_version())?;
        module.add("__api_version__", 1_u32)?;
        let mut names: Vec<String> = module
            .dict()
            .keys()
            .iter()
            .filter_map(|name| name.extract::<String>().ok())
            .filter(|name| {
                !name.starts_with('_')
                    || matches!(
                        name.as_str(),
                        "__version__" | "__symbolica_version__" | "__api_version__"
                    )
            })
            .collect();
        names.sort();
        module.add("__all__", names)?;
        Ok(())
    }

    fn initialize(_py: Python<'_>) -> PyResult<()> {
        Ok(())
    }

    fn get_citations() -> Vec<Citation> {
        if !USED.load(Ordering::Relaxed) {
            return Vec::new();
        }
        vec![Citation {
            id: "https://github.com/benruijl/hyperbolica".into(),
            reference: "Ben Ruijl. Hyperbolica (2026).".into(),
            bibtex: r#"@software{hyperbolica, author = {Ruijl, Ben}, title = {Hyperbolica}, year = {2026}, url = {https://github.com/benruijl/hyperbolica}}"#.into(),
            reasons: vec!["Exact definite integration using hyperlogarithms.".into()],
            description: String::new(), relevance: None,
        }]
    }
}

/// Exceptions are not collected by PyO3's class inventory.
#[cfg(feature = "python_stubgen")]
pub const STUB_EXTRAS: &str = include_str!("integration_extras.pyi");
