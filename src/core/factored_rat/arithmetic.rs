//! Arithmetic that preserves the factored denominator representation.

use super::{Factor, FactoredRat, exponent_as_usize};
use crate::core::Poly;
use crate::error::{Error, Result};

impl FactoredRat {
    pub fn negated(&self) -> Self {
        Self {
            numerator: -&self.numerator,
            den_factors: self.den_factors.clone(),
        }
    }

    pub fn neg(&self) -> Self {
        self.negated()
    }

    pub fn try_add(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        if self.is_zero() {
            return Ok(other.clone());
        }
        if other.is_zero() {
            return Ok(self.clone());
        }

        // The common factored denominator uses the maximum exponent for each
        // equal base in the ordered union of both lists.
        let mut common = self.den_factors.clone();
        for factor in &other.den_factors {
            if let Some(existing) = common
                .iter_mut()
                .find(|candidate| candidate.base.equal(&factor.base))
            {
                existing.exp = existing.exp.max(factor.exp);
            } else {
                common.push(factor.clone());
            }
        }

        let lift = |mut numerator: Poly, own_factors: &[Factor]| -> Result<Poly> {
            for factor in &common {
                let own_exponent = own_factors
                    .iter()
                    .find(|candidate| candidate.base.equal(&factor.base))
                    .map_or(0, |candidate| candidate.exp);
                let missing = factor.exp - own_exponent;
                if missing > 0 {
                    let power = factor.base.pow(exponent_as_usize(missing)?);
                    numerator = numerator.try_mul(&power)?;
                }
            }
            Ok(numerator)
        };

        let left = lift(self.numerator.clone(), &self.den_factors)?;
        let right = lift(other.numerator.clone(), &other.den_factors)?;
        Ok(Self {
            numerator: left.try_add(&right)?,
            den_factors: common,
        })
    }

    pub fn add(&self, other: &Self) -> Result<Self> {
        self.try_add(other)
    }

    pub fn try_sub(&self, other: &Self) -> Result<Self> {
        self.try_add(&other.negated())
    }

    pub fn sub(&self, other: &Self) -> Result<Self> {
        self.try_sub(other)
    }

    pub fn try_mul(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        if self.is_zero() || other.is_zero() {
            return Ok(Self::from_poly(Poly::zero(self.ctx().clone())));
        }

        let mut result = Self {
            numerator: self.numerator.try_mul(&other.numerator)?,
            den_factors: self.den_factors.clone(),
        };
        for factor in &other.den_factors {
            result.push_factor(&factor.base, factor.exp)?;
        }
        Ok(result)
    }

    pub fn mul(&self, other: &Self) -> Result<Self> {
        self.try_mul(other)
    }

    pub fn reciprocal(&self) -> Result<Self> {
        if self.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let mut result = Self::from_poly(self.expand_denominator()?);
        if !self.numerator.is_one() {
            result.push_factor(&self.numerator, 1)?;
        }
        Ok(result)
    }

    pub fn try_div(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        self.try_mul(&other.reciprocal()?)
    }

    pub fn div(&self, other: &Self) -> Result<Self> {
        self.try_div(other)
    }

    pub fn pow(&self, exponent: i64) -> Result<Self> {
        if exponent == 0 {
            return Ok(Self::from_poly(Poly::one(self.ctx().clone())));
        }
        if exponent < 0 {
            let magnitude = exponent
                .checked_abs()
                .ok_or(Error::InvalidExponent(exponent))?;
            return self.reciprocal()?.pow_positive(magnitude);
        }
        self.pow_positive(exponent)
    }

    fn pow_positive(&self, exponent: i64) -> Result<Self> {
        debug_assert!(exponent > 0);
        let mut factors = Vec::with_capacity(self.den_factors.len());
        for factor in &self.den_factors {
            let exp = factor.exp.checked_mul(exponent).ok_or_else(|| {
                Error::InvalidInput("denominator factor exponent overflow".into())
            })?;
            factors.push(Factor {
                base: factor.base.clone(),
                exp,
            });
        }
        Ok(Self {
            numerator: self.numerator.pow(exponent_as_usize(exponent)?),
            den_factors: factors,
        })
    }

    /// Differentiate while preserving the factored denominator.
    pub fn derivative(&self, variable: usize) -> Result<Self> {
        let numerator_derivative = self.numerator.derivative(variable)?;
        if self.den_factors.is_empty() {
            return Ok(Self::from_poly(numerator_derivative));
        }

        // A denominator independent of this variable needs no exponent bump.
        let mut any_dependent = false;
        for factor in &self.den_factors {
            if factor.base.degree(variable)? > 0 {
                any_dependent = true;
                break;
            }
        }
        if !any_dependent {
            return Ok(Self {
                numerator: numerator_derivative,
                den_factors: self.den_factors.clone(),
            });
        }

        // With P = product(f_i), the quotient-rule numerator over
        // product(f_i^(e_i+1)) is
        //   N' P - N sum_i(e_i f_i' product_{j != i}(f_j)).
        // Prefix/suffix products avoid rebuilding each product from scratch.
        let one = Poly::one(self.ctx().clone());
        let factor_count = self.den_factors.len();
        let mut prefixes = Vec::with_capacity(factor_count + 1);
        prefixes.push(one.clone());
        for factor in &self.den_factors {
            prefixes.push(prefixes.last().unwrap().try_mul(&factor.base)?);
        }

        let mut suffixes = vec![one; factor_count + 1];
        for index in (0..factor_count).rev() {
            suffixes[index] = self.den_factors[index].base.try_mul(&suffixes[index + 1])?;
        }

        let term_one = numerator_derivative.try_mul(&prefixes[factor_count])?;
        let mut logarithmic_numerator = Poly::zero(self.ctx().clone());
        for (index, factor) in self.den_factors.iter().enumerate() {
            let factor_derivative = factor.base.derivative(variable)?;
            if factor_derivative.is_zero() {
                continue;
            }
            let product_without_factor = prefixes[index].try_mul(&suffixes[index + 1])?;
            let weighted_derivative = Poly::from_int(self.ctx().clone(), factor.exp)
                .try_mul(&factor_derivative)?
                .try_mul(&product_without_factor)?;
            logarithmic_numerator = logarithmic_numerator.try_add(&weighted_derivative)?;
        }
        let term_two = self.numerator.try_mul(&logarithmic_numerator)?;

        let mut denominator = Vec::with_capacity(factor_count);
        for factor in &self.den_factors {
            denominator.push(Factor {
                base: factor.base.clone(),
                exp: factor.exp.checked_add(1).ok_or_else(|| {
                    Error::InvalidInput("denominator factor exponent overflow".into())
                })?,
            });
        }
        Ok(Self {
            numerator: term_one.try_sub(&term_two)?,
            den_factors: denominator,
        })
    }
}
