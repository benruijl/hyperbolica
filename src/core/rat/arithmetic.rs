//! Native rational-function arithmetic.

use std::ops::Neg;

use symbolica::domains::rational_polynomial::FromNumeratorAndDenominator;
use symbolica::prelude::Z;

use super::{NativeRat, Rat};
use crate::error::{Error, Result};

impl Rat {
    pub fn try_add(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        if other.is_zero() {
            return Ok(self.clone());
        }
        if self.is_zero() {
            return Ok(other.clone_in_context(&self.ctx));
        }
        Self::from_native(
            self.ctx.clone(),
            self.native.as_ref() + other.native.as_ref(),
        )
    }

    pub fn try_sub(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        if other.is_zero() {
            return Ok(self.clone());
        }
        if self.is_zero() {
            return Ok(other.negated().clone_in_context(&self.ctx));
        }
        Self::from_native(
            self.ctx.clone(),
            self.native.as_ref() - other.native.as_ref(),
        )
    }

    pub fn try_mul(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        if self.is_zero() || other.is_one() {
            return Ok(self.clone());
        }
        if other.is_zero() || self.is_one() {
            return Ok(other.clone_in_context(&self.ctx));
        }
        Self::from_native(
            self.ctx.clone(),
            self.native.as_ref() * other.native.as_ref(),
        )
    }

    pub fn try_div(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        if other.is_zero() {
            return Err(Error::DivisionByZero);
        }
        if self.is_zero() || other.is_one() {
            return Ok(self.clone());
        }
        Self::from_native(
            self.ctx.clone(),
            self.native.as_ref() / other.native.as_ref(),
        )
    }

    pub fn negated(&self) -> Self {
        if self.is_zero() {
            return self.clone();
        }
        Self::from_native(self.ctx.clone(), self.native.as_ref().clone().neg())
            .expect("negation preserves a rational function's context")
    }

    pub fn pow(&self, exponent: i64) -> Result<Self> {
        let magnitude = exponent
            .checked_abs()
            .ok_or(Error::InvalidExponent(exponent))? as u64;
        if magnitude > u64::from(u32::MAX) {
            return Err(Error::InvalidExponent(exponent));
        }
        if exponent == 1 || self.is_one() {
            return Ok(self.clone());
        }
        if exponent < 0 && self.is_zero() {
            return Err(Error::DivisionByZero);
        }
        // Canonical numerator and denominator are coprime. Their powers
        // remain coprime, so generic rational squaring would only repeat
        // already-proven GCDs and exact divisions at every bit of the power.
        let numerator = self.native.numerator.pow(magnitude as usize);
        let denominator = self.native.denominator.pow(magnitude as usize);
        let native = if exponent >= 0 {
            NativeRat {
                numerator,
                denominator,
            }
        } else {
            // Swapping a negative numerator into the denominator still
            // requires the canonical sign normalization, but no GCD.
            NativeRat::from_num_den(denominator, numerator, &Z, false)
        };
        Self::from_native(self.ctx.clone(), native)
    }

    pub fn derivative(&self, variable: usize) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        Self::from_native(self.ctx.clone(), self.native.derivative(variable))
    }
}
