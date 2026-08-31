use std::fmt::{Display, Formatter};

use crate::core::{FactoredRat, Rat, SymCoef};
use crate::error::Error;
use crate::symbols::Word;

mod contour;
mod driver;
mod entry;

#[cfg(test)]
mod tests;

pub use contour::close_positive_letters;
pub(super) use driver::integration_step_core_sym_with_options;
pub use driver::{
    integration_step, integration_step_sym, integration_step_sym_with_options,
    integration_step_with_options,
};

/// One rational coefficient times a shuffle product of words.
#[derive(Clone, Debug)]
pub struct ShuffleEntry {
    pub coef: Rat,
    pub shuffle: Vec<Word>,
    /// Optional deferred representation for a bare rational integrand.
    /// The current Symbolica partial-fraction layer materializes it once at
    /// the entry boundary; retaining the field keeps the API compatible with
    /// the stay-factored optimization path.
    pub factored_den: Option<FactoredRat>,
}

impl ShuffleEntry {
    pub fn new(coef: Rat, shuffle: Vec<Word>) -> Self {
        Self {
            coef,
            shuffle,
            factored_den: None,
        }
    }
}

pub type ShuffleList = Vec<ShuffleEntry>;

#[derive(Clone, Debug)]
pub struct ShuffleEntrySym {
    pub coef: SymCoef,
    pub shuffle: Vec<Word>,
    pub factored_den: Option<FactoredRat>,
}

impl ShuffleEntrySym {
    pub fn new(coef: SymCoef, shuffle: Vec<Word>) -> Self {
        Self {
            coef,
            shuffle,
            factored_den: None,
        }
    }
}

pub type ShuffleListSym = Vec<ShuffleEntrySym>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boundary {
    Zero,
    Infinity,
}

impl Display for Boundary {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Zero => "zero",
            Self::Infinity => "infinity",
        })
    }
}

/// Failures with semantic meaning at the integration-pipeline boundary.
#[derive(Debug, thiserror::Error)]
pub enum IntegrationError {
    #[error(transparent)]
    Algebra(#[from] Error),

    #[error(
        "HyperFLINT: divergent integral at {boundary} for `{variable}`: \
         Log[{variable}]^{log_power} / {variable}^{power}"
    )]
    Divergent {
        boundary: Boundary,
        variable: String,
        log_power: i64,
        power: i64,
    },
}

pub type IntegrationResult<T> = std::result::Result<T, IntegrationError>;

#[derive(Clone, Debug)]
pub struct IntegrationStepOptions {
    pub check_divergences: bool,
    /// Parallelize four or more independent shuffle entries. Indexed Rayon
    /// collection followed by a serial merge keeps output deterministic.
    pub parallel: bool,
    pub introduce_algebraic_letters: bool,
}

impl Default for IntegrationStepOptions {
    fn default() -> Self {
        Self {
            check_divergences: false,
            parallel: true,
            introduce_algebraic_letters: false,
        }
    }
}
