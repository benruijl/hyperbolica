//! Native rational-function arithmetic.

use std::ops::Neg;

use symbolica::prelude::{EuclideanDomain, Ring, Z};

use super::{NativeRat, Rat};
use crate::error::{Error, Result};

fn remove_common_integer_content(mut value: NativeRat) -> NativeRat {
    let content = Z.gcd(&value.numerator.content(), &value.denominator.content());
    if !Z.is_one(&content) {
        value.numerator = value.numerator.div_coeff(&content);
        value.denominator = value.denominator.div_coeff(&content);
    }
    value
}

impl Rat {
    pub fn try_add(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        Self::from_native(
            self.ctx.clone(),
            self.native.as_ref() + other.native.as_ref(),
        )
    }

    pub fn try_sub(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        Self::from_native(
            self.ctx.clone(),
            self.native.as_ref() - other.native.as_ref(),
        )
    }

    pub fn try_mul(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
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
        Self::from_native(
            self.ctx.clone(),
            self.native.as_ref() / other.native.as_ref(),
        )
    }

    pub fn negated(&self) -> Self {
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
        let native = if exponent >= 0 {
            self.native.pow(magnitude)
        } else {
            if self.is_zero() {
                return Err(Error::DivisionByZero);
            }
            self.native.as_ref().clone().inv().pow(magnitude)
        };
        Self::from_native(self.ctx.clone(), native)
    }

    pub fn derivative(&self, variable: usize) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        // Symbolica's derivative constructs its quotient-rule intermediates
        // with `do_gcd = false`. Integer differentiation can introduce a
        // common scalar even when the input itself is canonical, so remove
        // that scalar before structural equality and hashing observe it.
        let derivative = remove_common_integer_content(self.native.derivative(variable));
        Self::from_native(self.ctx.clone(), derivative)
    }
}
