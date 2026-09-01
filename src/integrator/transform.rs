//! Transform hyperlogarithm words into shuffle factors and regulators.

use crate::core::{Rat, SymCoef};
use crate::symbols::{Word, Wordlist};

mod collection;
mod limits;
mod shuffle;
mod word;

#[cfg(test)]
mod tests;

pub(crate) use collection::regkey_structural_cmp;
pub use collection::{
    canonicalize_regkey, canonicalize_regulator, canonicalize_regulator_sym, collect_regulator,
    collect_regulator_sym, regkey_content_key, regulator_content_key, regulator_sym_content_key,
    shuffle_symbolic, shuffle_symbolic_sym,
};
pub use limits::{reglim_word, reglim_word_with_table};
pub(crate) use shuffle::transform_shuffle_with_options_and_table;
pub use shuffle::{transform_shuffle, transform_shuffle_with_options};
pub(crate) use word::transform_word_with_options_and_table;
pub use word::{transform_word, transform_word_with_options};

/// Algebraic-letter policy for word and shuffle transformation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransformOptions<'a> {
    /// Split eligible irreducible quadratics into registered Wm/Wp roots.
    pub introduce_algebraic_letters: bool,
    /// Later integration variables that an introduced root must not depend on.
    pub forbidden_variables: &'a [usize],
}

/// A commutative product of symbolic hyperlogarithm periods.
pub type RegKey = Vec<Word>;

/// One rational term in a symbolic regulator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegTerm {
    pub coef: Rat,
    pub key: RegKey,
}

pub type Regulator = Vec<RegTerm>;

/// Symbolic-coefficient variant used by the integration pipeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegTermSym {
    pub coef: SymCoef,
    pub key: RegKey,
}

pub type RegulatorSym = Vec<RegTermSym>;

/// A transformed word is a shuffle expression times a regulator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransformPair {
    pub shuffle: Wordlist,
    pub regulator: RegulatorSym,
}

pub type TransformResult = Vec<TransformPair>;
