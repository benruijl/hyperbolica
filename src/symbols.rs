//! Hyperbolica's typed words and centrally registered Symbolica heads.
//!
//! Every non-built-in symbol introduced by the library is registered here.
//! Algorithm modules must obtain these heads through [`heads`] instead of
//! creating ad-hoc symbols or relying on parser side effects.

mod hlog;
mod hooks;
pub(crate) mod legacy;
mod mpl;
mod word;

use symbolica::prelude::{Atom, AtomCore, AtomView, Symbol, get_symbol, initialize, symbol};

pub use hlog::Hlog;
pub use mpl::Mpl;
pub use word::{Letter, Word, Wordlist, WordlistTerm};

pub const SYMBOL_NAMESPACE: &str = "hyperbolica";

pub const HLOG_NAME: &str = "hyperbolica::Hlog";
pub const MPL_NAME: &str = "hyperbolica::Mpl";
pub const MZV_NAME: &str = "hyperbolica::MZV";
pub const DELTA_NAME: &str = "hyperbolica::delta";
pub const PERIOD_NAME: &str = "hyperbolica::Period";
pub const ALGEBRAIC_MINUS_NAME: &str = "hyperbolica::Wm";
pub const ALGEBRAIC_PLUS_NAME: &str = "hyperbolica::Wp";
pub const ALGEBRAIC_RATIO_NAME: &str = "hyperbolica::WmOverWp";
pub const SQRT_DISCRIMINANT_NAME: &str = "hyperbolica::sqrt_disc";
pub const LOG_TWO_NAME: &str = "hyperbolica::Log2";

initialize!(|| {
    let _ = symbol!(
        HLOG_NAME,
        norm = hooks::normalize_hlog,
        der = hooks::differentiate_hlog,
        series = hooks::series_hlog
    );
    let _ = symbol!(
        MPL_NAME,
        norm = hooks::normalize_mpl,
        der = hooks::differentiate_mpl,
        series = hooks::series_mpl
    );
    let _ = symbol!(
        MZV_NAME,
        DELTA_NAME,
        PERIOD_NAME,
        ALGEBRAIC_MINUS_NAME,
        ALGEBRAIC_PLUS_NAME,
        ALGEBRAIC_RATIO_NAME,
        SQRT_DISCRIMINANT_NAME,
        LOG_TWO_NAME
    );
});

/// The complete set of Symbolica heads owned by Hyperbolica.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HyperbolicaSymbols {
    pub hlog: Symbol,
    pub mpl: Symbol,
    pub mzv: Symbol,
    pub delta: Symbol,
    pub period: Symbol,
    pub algebraic_minus: Symbol,
    pub algebraic_plus: Symbol,
    pub algebraic_ratio: Symbol,
    pub sqrt_discriminant: Symbol,
    pub log_two: Symbol,
}

/// Fetch the centrally initialized Symbolica heads.
pub fn heads() -> HyperbolicaSymbols {
    HyperbolicaSymbols {
        hlog: get_symbol!(HLOG_NAME).expect("Hyperbolica Hlog head was not initialized"),
        mpl: get_symbol!(MPL_NAME).expect("Hyperbolica Mpl head was not initialized"),
        mzv: get_symbol!(MZV_NAME).expect("Hyperbolica MZV head was not initialized"),
        delta: get_symbol!(DELTA_NAME).expect("Hyperbolica delta head was not initialized"),
        period: get_symbol!(PERIOD_NAME).expect("Hyperbolica Period head was not initialized"),
        algebraic_minus: get_symbol!(ALGEBRAIC_MINUS_NAME)
            .expect("Hyperbolica Wm head was not initialized"),
        algebraic_plus: get_symbol!(ALGEBRAIC_PLUS_NAME)
            .expect("Hyperbolica Wp head was not initialized"),
        algebraic_ratio: get_symbol!(ALGEBRAIC_RATIO_NAME)
            .expect("Hyperbolica WmOverWp head was not initialized"),
        sqrt_discriminant: get_symbol!(SQRT_DISCRIMINANT_NAME)
            .expect("Hyperbolica discriminant head was not initialized"),
        log_two: get_symbol!(LOG_TWO_NAME).expect("Hyperbolica Log2 head was not initialized"),
    }
}

/// The four registered function indeterminates associated with one quadratic
/// algebraic-letter pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlgebraicAtoms {
    pub minus: Atom,
    pub plus: Atom,
    pub ratio: Atom,
    pub sqrt_discriminant: Atom,
}

/// Construct an MZV indeterminate without minting a dynamic symbol.
pub fn mzv_atom(indices: &[i64]) -> Atom {
    heads().mzv.call_args(indices.iter().copied())
}

/// Construct all atoms for a one-based algebraic-letter pair.
pub fn algebraic_atoms(index: u32) -> AlgebraicAtoms {
    let symbols = heads();
    AlgebraicAtoms {
        minus: symbols.algebraic_minus.call(index),
        plus: symbols.algebraic_plus.call(index),
        ratio: symbols.algebraic_ratio.call(index),
        sqrt_discriminant: symbols.sqrt_discriminant.call(index),
    }
}

/// Construct a contour-discontinuity factor for a native indeterminate.
pub fn delta_atom(variable: impl AtomCore) -> Atom {
    heads().delta.call(variable.as_atom_view())
}

/// Construct an opaque period indeterminate.
pub fn period_atom(index: u32) -> Atom {
    heads().period.call(index)
}

/// Return the centrally registered `Log2` atom.
pub fn log_two_atom() -> Atom {
    heads().log_two.to_atom()
}

/// Whether an indeterminate is a constant owned by Hyperbolica.
///
/// This is intentionally structural: reduction-table membership is not the
/// definition of an `MZV(...)` or algebraic-letter constant. In particular,
/// unknown-yet-valid MZVs and algebraic atoms restored from bridge output must
/// never be treated as spectator kinematic variables.
pub(crate) fn is_library_constant(atom: AtomView<'_>) -> bool {
    let symbols = heads();
    match atom {
        AtomView::Var(variable) => variable.get_symbol() == symbols.log_two,
        AtomView::Fun(function) => matches!(
            function.get_symbol(),
            symbol
                if symbol == symbols.mzv
                    || symbol == symbols.delta
                    || symbol == symbols.period
                    || symbol == symbols.algebraic_minus
                    || symbol == symbols.algebraic_plus
                    || symbol == symbols.algebraic_ratio
                    || symbol == symbols.sqrt_discriminant
        ),
        AtomView::Num(_) | AtomView::Pow(_) | AtomView::Mul(_) | AtomView::Add(_) => false,
    }
}

#[cfg(test)]
mod initialization_tests {
    use super::*;

    #[test]
    fn every_library_owned_head_is_initialized_in_one_namespace() {
        let symbols = heads();
        let expected = [
            HLOG_NAME,
            MPL_NAME,
            MZV_NAME,
            DELTA_NAME,
            PERIOD_NAME,
            ALGEBRAIC_MINUS_NAME,
            ALGEBRAIC_PLUS_NAME,
            ALGEBRAIC_RATIO_NAME,
            SQRT_DISCRIMINANT_NAME,
            LOG_TWO_NAME,
        ];
        let actual = [
            symbols.hlog,
            symbols.mpl,
            symbols.mzv,
            symbols.delta,
            symbols.period,
            symbols.algebraic_minus,
            symbols.algebraic_plus,
            symbols.algebraic_ratio,
            symbols.sqrt_discriminant,
            symbols.log_two,
        ];
        for (symbol, expected_name) in actual.into_iter().zip(expected) {
            assert_eq!(symbol.get_name(), expected_name);
        }
    }

    #[test]
    fn parameterized_constants_reuse_only_registered_heads() {
        let mzv = mzv_atom(&[-2, 3]);
        assert_eq!(mzv.as_fun_view().unwrap().get_symbol(), heads().mzv);

        let algebraic = algebraic_atoms(7);
        assert_eq!(
            algebraic.minus.as_fun_view().unwrap().get_symbol(),
            heads().algebraic_minus
        );
        assert_eq!(
            algebraic.plus.as_fun_view().unwrap().get_symbol(),
            heads().algebraic_plus
        );
        assert_eq!(
            algebraic.ratio.as_fun_view().unwrap().get_symbol(),
            heads().algebraic_ratio
        );
        assert_eq!(
            algebraic
                .sqrt_discriminant
                .as_fun_view()
                .unwrap()
                .get_symbol(),
            heads().sqrt_discriminant
        );

        assert!(matches!(
            log_two_atom().as_view(),
            symbolica::prelude::AtomView::Var(_)
        ));
        assert_eq!(
            period_atom(3).as_fun_view().unwrap().get_symbol(),
            heads().period
        );
    }

    #[test]
    fn owned_constant_classification_is_structural_not_table_driven() {
        assert!(is_library_constant(mzv_atom(&[999]).as_view()));
        assert!(is_library_constant(algebraic_atoms(91).minus.as_view()));
        assert!(is_library_constant(log_two_atom().as_view()));
        assert!(!is_library_constant(heads().hlog.call(1).as_view()));
    }
}
