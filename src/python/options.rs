use pyo3::{pyclass, pymethods};

use crate::{
    api::AtomIntegrationOptions,
    reduce::{MzvReductionRule, MzvReductionTable},
};

/// Options controlling exact hyperlogarithm integration.
///
/// Reduction rules are `(left_hand_side, right_hand_side)` string pairs in
/// HyperFLINT's generated MZV-table notation.  They are data-table entries,
/// not mathematical input expressions passed through the Python boundary.
///
/// Instances are mutable, compare by value, and support `copy.copy` and
/// `copy.deepcopy`. Preparing an input stores an independent copy of the
/// options, so later mutations do not affect that prepared object.
#[pyclass(
    from_py_object,
    module = "hyperbolica",
    name = "IntegrationOptions",
    eq
)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PythonIntegrationOptions {
    inner: AtomIntegrationOptions,
}

impl PythonIntegrationOptions {
    pub(crate) fn from_rust(inner: AtomIntegrationOptions) -> Self {
        Self { inner }
    }

    pub(crate) fn to_rust(&self) -> AtomIntegrationOptions {
        self.inner.clone()
    }
}

#[pymethods]
impl PythonIntegrationOptions {
    #[new]
    #[pyo3(signature = (
        *,
        check_divergences = false,
        parallel = true,
        introduce_algebraic_letters = false,
        close_final_positive_letters = true,
        mzv_reductions = None,
        mzv_basis = None
    ), text_signature = "(*, check_divergences=False, parallel=True, introduce_algebraic_letters=False, close_final_positive_letters=True, mzv_reductions=None, mzv_basis=None)")]
    fn new(
        check_divergences: bool,
        parallel: bool,
        introduce_algebraic_letters: bool,
        close_final_positive_letters: bool,
        mzv_reductions: Option<Vec<(String, String)>>,
        mzv_basis: Option<Vec<String>>,
    ) -> Self {
        let reductions = mzv_reductions
            .unwrap_or_default()
            .into_iter()
            .map(|(lhs, rhs)| MzvReductionRule { lhs, rhs })
            .collect();
        Self {
            inner: AtomIntegrationOptions {
                check_divergences,
                parallel,
                introduce_algebraic_letters,
                close_final_positive_letters,
                mzv_reductions: MzvReductionTable {
                    reductions,
                    basis: mzv_basis.unwrap_or_default(),
                },
            },
        }
    }

    /// Whether to detect non-cancelling endpoint divergences.
    #[getter]
    fn check_divergences(&self) -> bool {
        self.inner.check_divergences
    }

    #[setter]
    fn set_check_divergences(&mut self, value: bool) {
        self.inner.check_divergences = value;
    }

    /// Whether to parallelize independent shuffle entries deterministically.
    #[getter]
    fn parallel(&self) -> bool {
        self.inner.parallel
    }

    #[setter]
    fn set_parallel(&mut self, value: bool) {
        self.inner.parallel = value;
    }

    /// Whether the linear-factor layer may introduce algebraic letters.
    #[getter]
    fn introduce_algebraic_letters(&self) -> bool {
        self.inner.introduce_algebraic_letters
    }

    #[setter]
    fn set_introduce_algebraic_letters(&mut self, value: bool) {
        self.inner.introduce_algebraic_letters = value;
    }

    /// Whether to close positive-real-axis letters after the last variable.
    #[getter]
    fn close_final_positive_letters(&self) -> bool {
        self.inner.close_final_positive_letters
    }

    #[setter]
    fn set_close_final_positive_letters(&mut self, value: bool) {
        self.inner.close_final_positive_letters = value;
    }

    /// Exact MZV reduction rules as `(left_hand_side, right_hand_side)` pairs.
    #[getter]
    fn mzv_reductions(&self) -> Vec<(String, String)> {
        self.inner
            .mzv_reductions
            .reductions
            .iter()
            .map(|rule| (rule.lhs.clone(), rule.rhs.clone()))
            .collect()
    }

    #[setter]
    fn set_mzv_reductions(&mut self, reductions: Vec<(String, String)>) {
        self.inner.mzv_reductions.reductions = reductions
            .into_iter()
            .map(|(lhs, rhs)| MzvReductionRule { lhs, rhs })
            .collect();
    }

    /// Symbolica constant names reserved as the active MZV basis.
    #[getter]
    fn mzv_basis(&self) -> Vec<String> {
        self.inner.mzv_reductions.basis.clone()
    }

    #[setter]
    fn set_mzv_basis(&mut self, basis: Vec<String>) {
        self.inner.mzv_reductions.basis = basis;
    }

    /// Return an independent options object.
    fn __copy__(&self) -> Self {
        self.clone()
    }

    /// Return an independent options object; all fields already own data.
    fn __deepcopy__(&self, _memo: &pyo3::Bound<'_, pyo3::PyAny>) -> Self {
        self.clone()
    }

    fn __repr__(&self) -> String {
        let reductions = self
            .inner
            .mzv_reductions
            .reductions
            .iter()
            .map(|rule| (rule.lhs.clone(), rule.rhs.clone()))
            .collect::<Vec<_>>();
        format!(
            "IntegrationOptions(check_divergences={}, parallel={}, \
             introduce_algebraic_letters={}, close_final_positive_letters={}, \
             mzv_reductions={reductions:?}, mzv_basis={:?})",
            python_bool(self.inner.check_divergences),
            python_bool(self.inner.parallel),
            python_bool(self.inner.introduce_algebraic_letters),
            python_bool(self.inner.close_final_positive_letters),
            self.inner.mzv_reductions.basis,
        )
    }
}

fn python_bool(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_defaults_exactly_match_the_atom_api() {
        assert_eq!(
            PythonIntegrationOptions::default().to_rust(),
            AtomIntegrationOptions::default()
        );
    }

    #[test]
    fn constructor_preserves_the_complete_mzv_table() {
        let options = PythonIntegrationOptions::new(
            true,
            false,
            false,
            true,
            Some(vec![("MZV3".into(), "zeta3".into())]),
            Some(vec!["zeta3".into()]),
        );
        let rust = options.to_rust();
        assert!(rust.check_divergences);
        assert!(!rust.parallel);
        assert_eq!(rust.mzv_reductions.reductions[0].lhs, "MZV3");
        assert_eq!(rust.mzv_reductions.basis, ["zeta3"]);
    }
}
