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
            reasons: vec!["Used through HEPkit to perform exact definite integration with hyperlogarithms.".into()],
            description: "Rust library for exact hyperlogarithmic integration, backed by Symbolica's symbolic algebra.".into(),
            relevance: None,
        }, Citation {
            id: "https://arxiv.org/abs/1403.3385".into(),
            reference: "Erik Panzer. Algorithms for the symbolic integration of hyperlogarithms with applications to Feynman integrals. Computer Physics Communications 188 (2015), 148–166.".into(),
            bibtex: r#"@article{Panzer:2014caa,
  author = {Panzer, Erik},
  title = {Algorithms for the symbolic integration of hyperlogarithms with applications to Feynman integrals},
  journal = {Computer Physics Communications},
  volume = {188},
  pages = {148--166},
  year = {2015},
  doi = {10.1016/j.cpc.2014.10.019},
  eprint = {1403.3385},
  archivePrefix = {arXiv},
  primaryClass = {hep-th}
}"#.into(),
            reasons: vec!["Hyperbolica's integration method builds on the algorithms developed for HyperInt: integration of rational functions times hyperlogarithms and regularized endpoint evaluation.".into()],
            description: "Maple package for symbolic integration of hyperlogarithms, with applications to linearly reducible Feynman integrals.".into(),
            relevance: None,
        }, Citation {
            id: "https://arxiv.org/abs/2604.20954".into(),
            reference: "Mathieu Giroux, Sebastian Mizera and Giulio Salvatori. SubTropica (2026), arXiv:2604.20954 [hep-th].".into(),
            bibtex: r#"@article{Giroux:2026tgd,
  author = {Giroux, Mathieu and Mizera, Sebastian and Salvatori, Giulio},
  title = {{SubTropica}},
  eprint = {2604.20954},
  archivePrefix = {arXiv},
  primaryClass = {hep-th},
  month = {4},
  year = {2026}
}"#.into(),
            reasons: vec![
                "Hyperbolica is a Rust port of SubTropica's HyperFLINT hyperlogarithmic integration backend.".into(),
                "SubTropica also supplies the upstream multiple-zeta-value reduction data and regression cases used by Hyperbolica.".into(),
            ],
            description: "Mathematica package for evaluating Euler and Feynman integrals using tropical subtraction and hyperlogarithmic integration.".into(),
            relevance: None,
        }]
    }
}

/// Exceptions are not collected by PyO3's class inventory.
#[cfg(feature = "python_stubgen")]
pub const STUB_EXTRAS: &str = include_str!("integration_extras.pyi");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn citations_credit_upstream_projects_only_after_usage() {
        assert!(CommunityModule::get_citations().is_empty());
        record_usage();
        let citations = CommunityModule::get_citations();
        let ids: Vec<_> = citations
            .iter()
            .map(|citation| citation.id.as_str())
            .collect();
        assert_eq!(
            ids,
            [
                "https://github.com/benruijl/hyperbolica",
                "https://arxiv.org/abs/1403.3385",
                "https://arxiv.org/abs/2604.20954",
            ]
        );
        for citation in &citations {
            assert!(!citation.reference.is_empty());
            assert!(!citation.description.is_empty());
            assert!(!citation.reasons.is_empty());
            assert!(citation.reasons.iter().all(|reason| !reason.is_empty()));
            assert!(citation.bibtex.starts_with('@'));
        }
        // Repeated use must not add duplicate citations or reasons.
        record_usage();
        let repeated = CommunityModule::get_citations();
        assert_eq!(repeated.len(), citations.len());
        for (first, next) in citations.iter().zip(repeated.iter()) {
            assert_eq!(first.id, next.id);
            assert_eq!(first.reasons, next.reasons);
        }
    }
}
