use std::fmt::{Display, Formatter};

use crate::core::{FactoredRat, Rat, SymCoef};
use crate::error::Error;
use crate::symbols::Word;

mod contour;
mod driver;
mod entry;
mod ordered_parallel;

#[cfg(test)]
mod tests;

pub use contour::close_positive_letters;
pub(super) use driver::integration_step_core_sym_with_options;
pub use driver::{
    integration_step, integration_step_sym, integration_step_sym_with_options,
    integration_step_sym_with_options_and_remaining_variables, integration_step_with_options,
    integration_step_with_options_and_remaining_variables,
};

/// One rational coefficient times a shuffle product of words.
#[derive(Clone, Debug)]
pub struct ShuffleEntry {
    /// The complete coefficient for an ordinary entry. Entries built with
    /// [`Self::from_factored`] keep this at one as an invariant sentinel; the
    /// complete coefficient then lives in `factored_coefficient` until the
    /// first partial-fraction step.
    pub coef: Rat,
    pub shuffle: Vec<Word>,
    factored_coefficient: Option<FactoredRat>,
}

impl ShuffleEntry {
    pub fn new(coef: Rat, shuffle: Vec<Word>) -> Self {
        Self {
            coef,
            shuffle,
            factored_coefficient: None,
        }
    }

    /// Construct an entry whose complete rational coefficient is held in
    /// deferred denominator blocks.
    ///
    /// `coef` is deliberately set to one and is not an additional multiplier.
    /// Integration rejects an entry if that public sentinel is later changed,
    /// so the two representations can never be silently combined or one
    /// silently ignored.
    pub fn from_factored(coefficient: FactoredRat, shuffle: Vec<Word>) -> Self {
        Self {
            coef: Rat::one(coefficient.ctx().clone()),
            shuffle,
            factored_coefficient: Some(coefficient),
        }
    }

    /// Return the deferred complete coefficient, when this is a factored
    /// entry. In that case [`Self::coef`] is required to remain one.
    pub fn factored_coefficient(&self) -> Option<&FactoredRat> {
        self.factored_coefficient.as_ref()
    }

    pub(crate) fn materialized_coefficient(&self) -> Result<Rat, Error> {
        let Some(coefficient) = &self.factored_coefficient else {
            return Ok(self.coef.clone());
        };
        if !self.coef.is_one() {
            return Err(Error::InvalidInput(
                "a factored ShuffleEntry requires its `coef` sentinel to remain one".into(),
            ));
        }
        coefficient.materialize()
    }
}

pub type ShuffleList = Vec<ShuffleEntry>;

#[derive(Clone, Debug)]
pub struct ShuffleEntrySym {
    pub coef: SymCoef,
    pub shuffle: Vec<Word>,
    pub(crate) factored_coefficient: Option<FactoredRat>,
}

impl ShuffleEntrySym {
    pub fn new(coef: SymCoef, shuffle: Vec<Word>) -> Self {
        Self {
            coef,
            shuffle,
            factored_coefficient: None,
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
    /// Parallelize four or more independent shuffle entries. A bounded rolling
    /// window overlaps processing with ordered incremental collection, keeping
    /// output deterministic without retaining every result.
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
