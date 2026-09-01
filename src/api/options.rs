use crate::integrator::{HyperIntOptions, IntegrationStepOptions};
use crate::reduce::MzvReductionTable;
use crate::{
    algebra::{DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE, build_algebraic_letter_atom_list},
    error::Result,
    reduce::{build_mzv_atom_list, build_mzv_basis_atom_list, standard_mzv_reductions},
};
use symbolica::prelude::{Atom, AtomView, CoefficientView};

/// One endpoint of a directed real integration interval.
///
/// Finite endpoints remain native Symbolica expressions.  The two infinity
/// variants are explicit so a symbol merely named `Infinity` can never be
/// mistaken for an endpoint sentinel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntegrationEndpoint {
    Finite(Atom),
    PositiveInfinity,
    NegativeInfinity,
}

impl IntegrationEndpoint {
    /// Construct a finite endpoint from a native Symbolica expression.
    pub fn finite(value: impl Into<Atom>) -> Self {
        Self::Finite(value.into())
    }

    /// Classify a native Symbolica expression as a finite or directed-infinite
    /// endpoint.
    ///
    /// Real `+Infinity` and `-Infinity` coefficients are accepted. Complex or
    /// non-real directed infinities are rejected because interval rescaling is
    /// defined only on the real line.
    pub fn try_from_atom(value: Atom) -> Result<Self> {
        if let AtomView::Num(number) = value.as_view()
            && let CoefficientView::Infinity(direction) = number.get_coeff_view()
        {
            return match direction {
                Some((real, imaginary))
                    if imaginary.is_zero() && !real.is_zero() && real.is_negative() =>
                {
                    Ok(Self::NegativeInfinity)
                }
                Some((real, imaginary)) if imaginary.is_zero() && !real.is_zero() => {
                    Ok(Self::PositiveInfinity)
                }
                _ => Err(crate::error::Error::InvalidInput(
                    "integration endpoints must be finite or real directed infinities".into(),
                )),
            };
        }
        Ok(Self::Finite(value))
    }

    /// Return the native expression when this endpoint is finite.
    pub fn as_finite(&self) -> Option<&Atom> {
        match self {
            Self::Finite(value) => Some(value),
            Self::PositiveInfinity | Self::NegativeInfinity => None,
        }
    }
}

/// A directed interval associated with one integration variable.
///
/// The supported endpoint combinations match HyperFLINT's interval rescaler:
/// finite-to-finite, finite-to-either-infinity, either-infinity-to-finite,
/// and the whole real line from negative to positive infinity.  The reverse
/// all-infinite interval is intentionally unsupported upstream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntegrationInterval {
    pub from: IntegrationEndpoint,
    pub to: IntegrationEndpoint,
}

impl IntegrationInterval {
    pub fn new(from: IntegrationEndpoint, to: IntegrationEndpoint) -> Self {
        Self { from, to }
    }

    /// Construct an interval with two finite native Symbolica endpoints.
    pub fn finite(from: impl Into<Atom>, to: impl Into<Atom>) -> Self {
        Self::new(
            IntegrationEndpoint::finite(from),
            IntegrationEndpoint::finite(to),
        )
    }

    /// The default integration interval `[0, +Infinity)`.
    pub fn zero_to_infinity() -> Self {
        Self::new(
            IntegrationEndpoint::finite(Atom::zero()),
            IntegrationEndpoint::PositiveInfinity,
        )
    }

    /// The whole real line `[-Infinity, +Infinity)`.
    pub fn real_line() -> Self {
        Self::new(
            IntegrationEndpoint::NegativeInfinity,
            IntegrationEndpoint::PositiveInfinity,
        )
    }
}

impl Default for IntegrationInterval {
    fn default() -> Self {
        Self::zero_to_infinity()
    }
}

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
    ///
    /// The default is Hyperbolica's embedded standard table. Set this field
    /// to [`MzvReductionTable::default`] for an explicit empty override.
    pub mzv_reductions: MzvReductionTable,
}

impl Default for AtomIntegrationOptions {
    fn default() -> Self {
        Self {
            check_divergences: false,
            parallel: true,
            introduce_algebraic_letters: false,
            close_final_positive_letters: true,
            mzv_reductions: standard_mzv_reductions(),
        }
    }
}

impl AtomIntegrationOptions {
    /// Registered constants that the selected pipeline may introduce after
    /// input lowering (period MZVs, `Log2`, and optionally algebraic roots).
    pub(crate) fn reserved_indeterminates(&self) -> Result<Vec<Atom>> {
        let mzvs = if self.mzv_reductions.is_embedded_standard() {
            // The standard table has hundreds of generated left-hand sides.
            // Period evaluation expands those constants eagerly into this
            // small basis, so they must not inflate every polynomial context.
            build_mzv_basis_atom_list(&self.mzv_reductions, Vec::new())?
        } else {
            build_mzv_atom_list(&self.mzv_reductions, Vec::new())?
        };
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
    use symbolica::prelude::{AtomCore, Coefficient};

    use crate::{
        reduce::{MzvReductionRule, MzvReductionTable},
        symbols::{heads, log_two_atom, mzv_atom},
    };

    use super::*;

    #[test]
    fn native_endpoints_classify_real_infinities_without_string_sentinels() {
        let positive = Atom::num(Coefficient::positive_infinity());
        let negative = -positive.clone();
        assert_eq!(
            IntegrationEndpoint::try_from_atom(positive).unwrap(),
            IntegrationEndpoint::PositiveInfinity
        );
        assert_eq!(
            IntegrationEndpoint::try_from_atom(negative).unwrap(),
            IntegrationEndpoint::NegativeInfinity
        );
        assert_eq!(
            IntegrationEndpoint::try_from_atom(Atom::num(7)).unwrap(),
            IntegrationEndpoint::Finite(Atom::num(7))
        );
        assert!(
            IntegrationEndpoint::try_from_atom(Atom::num(Coefficient::complex_infinity())).is_err()
        );
    }

    #[test]
    fn interval_constructors_preserve_direction() {
        assert_eq!(
            IntegrationInterval::default(),
            IntegrationInterval::new(
                IntegrationEndpoint::Finite(Atom::zero()),
                IntegrationEndpoint::PositiveInfinity,
            )
        );
        assert_eq!(
            IntegrationInterval::real_line(),
            IntegrationInterval::new(
                IntegrationEndpoint::NegativeInfinity,
                IntegrationEndpoint::PositiveInfinity,
            )
        );
        assert_eq!(
            IntegrationInterval::finite(2, 5),
            IntegrationInterval::new(
                IntegrationEndpoint::Finite(Atom::num(2)),
                IntegrationEndpoint::Finite(Atom::num(5)),
            )
        );
    }

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
        assert!(public.mzv_reductions.is_embedded_standard());
        assert!(!public.mzv_reductions.reductions().is_empty());
    }

    #[test]
    fn standard_default_is_basis_only_but_explicit_empty_remains_empty() {
        let standard = AtomIntegrationOptions::default();
        let reserved = standard.reserved_indeterminates().unwrap();
        assert_eq!(reserved.len(), standard.mzv_reductions.basis().len());
        assert!(reserved.len() < standard.mzv_reductions.reductions().len());

        let empty = AtomIntegrationOptions {
            mzv_reductions: MzvReductionTable::default(),
            ..AtomIntegrationOptions::default()
        };
        assert!(empty.mzv_reductions.is_empty());
        assert_eq!(empty.reserved_indeterminates().unwrap(), [log_two_atom()]);
    }

    #[test]
    fn reserved_constants_use_only_registered_atoms() {
        let options = AtomIntegrationOptions {
            introduce_algebraic_letters: true,
            mzv_reductions: MzvReductionTable::from_parts(
                vec![MzvReductionRule {
                    lhs: "mzv_4".into(),
                    rhs: "2/5*mzv_2^2".into(),
                }],
                vec!["Log2".into(), "mzv_2".into()],
            ),
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
