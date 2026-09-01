//! Rational functions with deferred denominator factor products.

mod apart;
mod arithmetic;
mod construction;
mod materialization;
mod traits;

#[cfg(test)]
mod tests;

use super::Poly;
use crate::error::{Error, Result};

/// One base in a [`FactoredRat`] denominator.
///
/// Exponents stored by `FactoredRat` are always strictly positive. Equal
/// bases are merged, preserving the order in which each base first appeared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Factor {
    pub base: Poly,
    pub exp: i64,
}

/// A rational function whose denominator remains a product of powers.
///
/// The represented value is
/// `numerator / product(factor.base ^ factor.exp)`. Keeping that product
/// factored avoids eagerly expanding the large denominator powers common in
/// HyperFLINT inputs. Reduction is deferred until [`Self::materialize`].
#[derive(Clone, Debug)]
pub struct FactoredRat {
    numerator: Poly,
    den_factors: Vec<Factor>,
}

impl FactoredRat {
    /// Below this term count, probing known factors generally costs more than
    /// letting the rational constructor perform its normal reduction.
    pub const PEEL_MIN_TERMS: usize = 64;
}

fn exponent_as_usize(exponent: i64) -> Result<usize> {
    usize::try_from(exponent).map_err(|_| Error::InvalidExponent(exponent))
}
