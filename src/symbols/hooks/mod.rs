//! Symbolica callbacks for Hyperbolica's expression heads.
//!
//! The callbacks deliberately recognize only the canonical normalized Atom
//! shapes documented in the child modules. Leaving a [`Settable`] untouched
//! (or returning `None` from a series callback) asks Symbolica to preserve its
//! generic derivative/series representation instead of guessing.

mod derivative;
mod series;
mod shape;

use symbolica::prelude::{Atom, AtomCore, AtomView};
use symbolica::utils::Settable;

pub(super) use derivative::{differentiate_hlog, differentiate_mpl};
pub(super) use series::{series_hlog, series_mpl};

/// `Hlog(z)` is the empty iterated integral and therefore the multiplicative
/// identity. Every non-empty word is left untouched.
pub(super) fn normalize_hlog(input: AtomView<'_>, output: &mut Settable<'_, Atom>) {
    if input
        .as_fun_view()
        .is_some_and(|function| function.get_nargs() == 1)
    {
        output.to_num(1);
    }
}

/// `Mpl()` is the depth-zero multiple polylogarithm and therefore one.
/// Positive-depth calls are deliberately not rewritten to classical
/// polylogarithms: the typed MPL head remains visible outside series calls.
pub(super) fn normalize_mpl(input: AtomView<'_>, output: &mut Settable<'_, Atom>) {
    if input
        .as_fun_view()
        .is_some_and(|function| function.get_nargs() == 0)
    {
        output.to_num(1);
    }
}
