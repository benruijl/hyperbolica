//! Denominator expansion, factor peeling, and canonical materialization.

use symbolica::domains::rational_polynomial::FromNumeratorAndDenominator;
use symbolica::prelude::{Field, Q, Ring, Z};

use super::apart::primitive_integer;
use super::{FactoredRat, exponent_as_usize};
use crate::core::{NativeRat, Poly, Rat};
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
    ///
    /// Clear rational content once per input polynomial. Products and exact
    /// factor peeling then run over integers, avoiding rational coefficient
    /// normalization at every multiplication or division step. Primitive
    /// integer factors divide over Q exactly when they divide over Z (Gauss's
    /// lemma), so the peeling optimization preserves the rational result.
    pub fn materialize(&self) -> Result<Rat> {
        self.materialize_with_peeling(Self::peel_enabled())
    }

    fn materialize_with_peeling(&self, enable_peeling: bool) -> Result<Rat> {
        if self.is_zero() {
            return Ok(Rat::zero(self.ctx().clone()));
        }
        if self.den_factors.is_empty() {
            return Ok(Rat::from_poly(self.numerator.clone()));
        }

        let peel = enable_peeling && self.numerator.n_terms() >= Self::PEEL_MIN_TERMS;
        let (mut numerator, mut scalar) = primitive_integer(&self.numerator);
        let mut denominator = numerator.one();
        for factor in &self.den_factors {
            let mut exponent = exponent_as_usize(factor.exp)?;
            let (base, unit) = primitive_integer(&factor.base);
            scalar = Q.div(&scalar, &Q.pow(&unit, exponent as u64));
            if base.is_one() {
                continue;
            }
            if peel {
                while exponent > 0 {
                    let Some(quotient) = numerator.try_div(&base) else {
                        break;
                    };
                    numerator = quotient;
                    exponent -= 1;
                }
            }
            if exponent > 0 {
                denominator = denominator * &base.pow(exponent);
            }
        }
        Rat::from_native(
            self.ctx().clone(),
            NativeRat::from_num_den(
                numerator.mul_coeff(scalar.numerator_ref().clone()),
                denominator.mul_coeff(scalar.denominator_ref().clone()),
                &Z,
                true,
            ),
        )
    }

    /// Upstream-compatible name for [`Self::materialize`].
    pub fn materialize_to_rat(&self) -> Result<Rat> {
        self.materialize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::PolyCtx;

    #[test]
    fn integer_materialization_preserves_rational_units_and_shared_factors() {
        let ctx = PolyCtx::new(["x", "y", "z"]).unwrap();
        let base = Poly::parse(ctx.clone(), "x+y+z+1").unwrap();
        // At least 64 terms: exercise both the peeled and unpeeled paths.
        let numerator = base
            .pow(7)
            .try_mul(&Poly::parse(ctx.clone(), "-7/15*(x+2)").unwrap())
            .unwrap();
        assert!(numerator.n_terms() >= FactoredRat::PEEL_MIN_TERMS);
        let mut value = FactoredRat::from_poly(numerator);
        for (factor, exponent) in [
            ("2/3*(x+y+z+1)", 2),
            ("-4/5*(x+y+z+1)", 3),
            ("6/7*(y+1)", 2),
            ("-3/11", 3),
        ] {
            value
                .push_factor(&Poly::parse(ctx.clone(), factor).unwrap(), exponent)
                .unwrap();
        }
        let oracle = Rat::new(
            value.numerator().clone(),
            value.expand_denominator().unwrap(),
        )
        .unwrap();
        assert_eq!(value.materialize_with_peeling(true).unwrap(), oracle);
        assert_eq!(value.materialize_with_peeling(false).unwrap(), oracle);
    }

    #[test]
    fn integer_materialization_matches_q_oracle_for_scaled_polynomials() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        for scale in [-7, -1, 1, 5] {
            for power in 1..=4 {
                let numerator =
                    Poly::parse(ctx.clone(), &format!("{scale}/6*(x+y+1)^3*(x-2*y)")).unwrap();
                let mut value = FactoredRat::from_poly(numerator);
                value
                    .push_factor(&Poly::parse(ctx.clone(), "-2/9*(x+y+1)").unwrap(), power)
                    .unwrap();
                value
                    .push_factor(&Poly::parse(ctx.clone(), "4/15*(x-y+3)").unwrap(), 2)
                    .unwrap();
                let oracle = Rat::new(
                    value.numerator().clone(),
                    value.expand_denominator().unwrap(),
                )
                .unwrap();
                assert_eq!(value.materialize_with_peeling(true).unwrap(), oracle);
                assert_eq!(value.materialize_with_peeling(false).unwrap(), oracle);
            }
        }
    }
}
