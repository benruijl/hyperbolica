use crate::integrator::{HyperIntOptions, IntegrationStepOptions};
use crate::reduce::MzvReductionTable;
use crate::{
    algebra::{DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE, build_algebraic_letter_atom_list},
    error::Result,
    reduce::build_mzv_atom_list,
};
use symbolica::prelude::Atom;

/// Configuration for an Atom-native integration.
///
/// All fields own their data and avoid Rust lifetimes, which keeps this type
/// straightforward to mirror in a future PyO3 class.  The reduction table is
/// supplied as data rather than loaded from a path, so the core API performs
/// no hidden filesystem I/O.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtomIntegrationOptions {
    /// Detect and report non-cancelling endpoint divergences.
    pub check_divergences: bool,
    /// Parallelize independent shuffle entries while retaining deterministic
    /// collection order.
    pub parallel: bool,
    /// Request algebraic-letter introduction in the linear-factor layer.
    pub introduce_algebraic_letters: bool,
    /// Close positive-real-axis letters after the last integration step.
    pub close_final_positive_letters: bool,
    /// Exact MZV reductions used by period and contour evaluation.
    pub mzv_reductions: MzvReductionTable,
}

impl Default for AtomIntegrationOptions {
    fn default() -> Self {
        Self {
            check_divergences: false,
            parallel: true,
            introduce_algebraic_letters: false,
            close_final_positive_letters: true,
            mzv_reductions: MzvReductionTable::default(),
        }
    }
}

impl AtomIntegrationOptions {
    /// Registered constants that the selected pipeline may introduce after
    /// input lowering (period MZVs, `Log2`, and optionally algebraic roots).
    pub(crate) fn reserved_indeterminates(&self) -> Result<Vec<Atom>> {
        let mzvs = build_mzv_atom_list(&self.mzv_reductions, Vec::new())?;
        Ok(if self.introduce_algebraic_letters {
            build_algebraic_letter_atom_list(mzvs, DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE)
        } else {
            mzvs
        })
    }

    pub(crate) fn core_options(&self) -> HyperIntOptions {
        HyperIntOptions {
            step: IntegrationStepOptions {
                check_divergences: self.check_divergences,
                parallel: self.parallel,
                introduce_algebraic_letters: self.introduce_algebraic_letters,
            },
            close_final_positive_letters: self.close_final_positive_letters,
        }
    }
}

#[cfg(test)]
mod tests {
    use symbolica::prelude::AtomCore;

    use crate::{
        reduce::{MzvReductionRule, MzvReductionTable},
        symbols::{heads, log_two_atom, mzv_atom},
    };

    use super::*;

    #[test]
    fn defaults_match_the_core_pipeline() {
        let public = AtomIntegrationOptions::default();
        let core = public.core_options();
        assert_eq!(core.step.check_divergences, public.check_divergences);
        assert_eq!(core.step.parallel, public.parallel);
        assert_eq!(
            core.step.introduce_algebraic_letters,
            public.introduce_algebraic_letters
        );
        assert_eq!(
            core.close_final_positive_letters,
            public.close_final_positive_letters
        );
    }

    #[test]
    fn reserved_constants_use_only_registered_atoms() {
        let options = AtomIntegrationOptions {
            introduce_algebraic_letters: true,
            mzv_reductions: MzvReductionTable {
                reductions: vec![MzvReductionRule {
                    lhs: "mzv_4".into(),
                    rhs: "2/5*mzv_2^2".into(),
                }],
                basis: vec!["Log2".into(), "mzv_2".into()],
            },
            ..AtomIntegrationOptions::default()
        };
        let atoms = options.reserved_indeterminates().unwrap();
        assert!(atoms.contains(&log_two_atom()));
        assert!(atoms.contains(&mzv_atom(&[2])));
        assert!(atoms.contains(&mzv_atom(&[4])));
        assert!(atoms.iter().any(|atom| {
            atom.as_fun_view()
                .is_some_and(|function| function.get_symbol() == heads().algebraic_minus)
        }));
        assert!(
            !atoms
                .iter()
                .any(|atom| atom.as_var_view().is_some_and(|variable| {
                    let symbol = variable.get_symbol();
                    let name = symbol.get_name();
                    name.contains("mzv_") || name.contains("Wm_")
                }))
        );
    }
}
