//! Denominator expansion, factor peeling, and canonical materialization.

use super::{FactoredRat, exponent_as_usize};
use crate::core::{Poly, Rat};
use crate::error::{Error, Result};

impl FactoredRat {
    /// Expand the denominator product to a single polynomial.
    pub fn expand_denominator(&self) -> Result<Poly> {
        let mut denominator = Poly::one(self.ctx().clone());
        for factor in &self.den_factors {
            denominator = denominator.try_mul(&factor.base.pow(exponent_as_usize(factor.exp)?))?;
        }
        Ok(denominator)
    }

    /// Alias for [`Self::expand_denominator`].
    pub fn expand(&self) -> Result<Poly> {
        self.expand_denominator()
    }

    /// Peel powers of known denominator bases from the numerator by exact
    /// division. Returns the number of removed powers.
    pub fn peel_known_factors(&mut self, min_terms: usize) -> Result<usize> {
        if self.numerator.is_zero() || self.numerator.n_terms() < min_terms {
            return Ok(0);
        }

        let mut removed = 0;
        for factor in &mut self.den_factors {
            while factor.exp > 0 {
                // `div_exact` already performs the divisibility test, so this
                // avoids upstream's duplicate divides-then-divide work.
                match self.numerator.div_exact(&factor.base) {
                    Ok(quotient) => {
                        self.numerator = quotient;
                        factor.exp -= 1;
                        removed += 1;
                    }
                    Err(Error::InexactDivision) => break,
                    Err(error) => return Err(error),
                }
            }
        }
        self.den_factors.retain(|factor| factor.exp > 0);
        Ok(removed)
    }

    /// Whether automatic peeling at materialization is enabled.
    /// `HF_FR_MAT_PEEL=0` opts out, matching upstream HyperFLINT.
    pub fn peel_enabled() -> bool {
        std::env::var_os("HF_FR_MAT_PEEL").is_none_or(|value| value != "0")
    }

    /// Expand and reduce to a canonical [`Rat`].
    pub fn materialize(&self) -> Result<Rat> {
        if self.is_zero() {
            return Ok(Rat::zero(self.ctx().clone()));
        }
        if self.den_factors.is_empty() {
            return Ok(Rat::from_poly(self.numerator.clone()));
        }

        if Self::peel_enabled() && self.numerator.n_terms() >= Self::PEEL_MIN_TERMS {
            let mut peeled = self.clone();
            peeled.peel_known_factors(Self::PEEL_MIN_TERMS)?;
            if peeled.den_factors.is_empty() {
                return Ok(Rat::from_poly(peeled.numerator));
            }
            return Rat::new(peeled.numerator.clone(), peeled.expand_denominator()?);
        }

        Rat::new(self.numerator.clone(), self.expand_denominator()?)
    }

    /// Upstream-compatible name for [`Self::materialize`].
    pub fn materialize_to_rat(&self) -> Result<Rat> {
        self.materialize()
    }
}
