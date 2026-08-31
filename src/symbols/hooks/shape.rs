//! Canonical structural encodings shared by the callbacks.

use symbolica::atom::representation::FunView;
use symbolica::prelude::{Atom, AtomView, ConvertToRing, Q, Symbol};

/// Split `Mpl(n1,...,nd,z1,...,zd)` after validating its positive integer
/// indices. Symbolica flattens `arg(...)` arguments during normalization, so
/// `Mpl(arg(n1,...),arg(z1,...))` reaches this same representation.
pub(super) fn split_mpl(function: FunView<'_>) -> Option<(Vec<i64>, Vec<AtomView<'_>>)> {
    let argument_count = function.get_nargs();
    if argument_count == 0 || !argument_count.is_multiple_of(2) {
        return None;
    }

    let depth = argument_count / 2;
    let arguments = function.iter().collect::<Vec<_>>();
    let indices = arguments[..depth]
        .iter()
        .map(|index| positive_integer(*index))
        .collect::<Option<Vec<_>>>()?;
    Some((indices, arguments[depth..].to_vec()))
}

pub(super) fn positive_integer(atom: AtomView<'_>) -> Option<i64> {
    let AtomView::Num(number) = atom else {
        return None;
    };
    let value = Q
        .try_element_from_coefficient_view(number.get_coeff_view())
        .ok()?;
    if !value.is_integer() || value <= 0 {
        return None;
    }
    value.numerator_ref().to_i64()
}

pub(super) fn hlog_atom(symbol: Symbol, endpoint: AtomView<'_>, word: &[AtomView<'_>]) -> Atom {
    if word.is_empty() {
        Atom::one()
    } else {
        symbol.call_args(std::iter::once(endpoint).chain(word.iter().copied()))
    }
}

pub(super) fn mpl_atom(symbol: Symbol, indices: &[i64], arguments: &[Atom]) -> Atom {
    if indices.is_empty() {
        Atom::one()
    } else {
        symbol.call_args(
            indices
                .iter()
                .copied()
                .map(Atom::num)
                .chain(arguments.iter().cloned()),
        )
    }
}
