//! Typed evaluation, substitution, integration, and Laurent operations.

use symbolica::domains::rational::RationalField;
use symbolica::domains::rational_polynomial::FromNumeratorAndDenominator;
use symbolica::prelude::*;

use super::{NativeRat, Rat};
use crate::error::{Error, Result};

type RationalPoly = MultivariatePolynomial<RationalField, u16>;

fn lift_native_polynomial(polynomial: &MultivariatePolynomial<IntegerRing, u16>) -> RationalPoly {
    polynomial.map_coeff(|coefficient| Q.to_element_numerator(coefficient.clone()), Q)
}

impl Rat {
    /// Substitute one variable by an exact rational number.
    ///
    /// Symbolica's integer-backed `RationalPolynomial` has mapped full
    /// evaluation but no partial rational replacement. Lift its two native
    /// integer polynomials to `Q`, call the typed polynomial `replace`, and
    /// normalize directly back to the native representation. The lazy public
    /// `Poly<Q>` compatibility views are never materialized on this path.
    pub fn substitute_rational(&self, variable: usize, value: &Rational) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if value.is_integer() {
            return self.substitute_integer(variable, value.numerator_ref());
        }
        let numerator = lift_native_polynomial(&self.native.numerator).replace(variable, value);
        let denominator = lift_native_polynomial(&self.native.denominator).replace(variable, value);
        if denominator.is_zero() {
            return Err(Error::DivisionByZero);
        }
        Self::from_native(
            self.ctx.clone(),
            NativeRat::from_num_den(numerator, denominator, &Z, true),
        )
    }

    /// Substitute one variable by an exact integer without leaving the native
    /// integer-polynomial representation.
    pub fn substitute_integer(&self, variable: usize, value: &Integer) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        let numerator = self.native.numerator.replace(variable, value);
        let denominator = self.native.denominator.replace(variable, value);
        if denominator.is_zero() {
            return Err(Error::DivisionByZero);
        }
        Self::from_native(
            self.ctx.clone(),
            NativeRat::from_num_den(numerator, denominator, &Z, true),
        )
    }

    /// Evaluate at a complete exact rational point.
    pub fn evaluate_rational(&self, values: &[Rational]) -> Result<Rational> {
        if values.len() != self.ctx.len() {
            return Err(Error::InvalidInput(format!(
                "expected {} evaluation values, got {}",
                self.ctx.len(),
                values.len()
            )));
        }
        let map_integer = |coefficient: &Integer| Q.to_element_numerator(coefficient.clone());
        let numerator = self
            .native
            .numerator
            .evaluate_with_coeff_map(map_integer, values, &Q);
        let denominator = self
            .native
            .denominator
            .evaluate_with_coeff_map(map_integer, values, &Q);
        if Q.is_zero(&denominator) {
            return Err(Error::DivisionByZero);
        }
        Ok(Q.div(&numerator, &denominator))
    }

    /// Evaluate at a complete exact integer point using native integer
    /// polynomial evaluation before constructing the final rational value.
    pub fn evaluate_integer(&self, values: &[Integer]) -> Result<Rational> {
        if values.len() != self.ctx.len() {
            return Err(Error::InvalidInput(format!(
                "expected {} evaluation values, got {}",
                self.ctx.len(),
                values.len()
            )));
        }
        let numerator = self.native.numerator.replace_all(values);
        let denominator = self.native.denominator.replace_all(values);
        if Z.is_zero(&denominator) {
            return Err(Error::DivisionByZero);
        }
        Ok(Q.to_element(numerator, denominator, true))
    }

    /// Integrate a rational function known to be polynomial in `variable`.
    ///
    /// This is the polynomial-part kernel used by the hyperlog primitive. It
    /// calls Symbolica's native polynomial `integrate` after one integer-to-Q
    /// coefficient lift and never materializes the lazy compatibility views.
    pub(crate) fn integrate_polynomial_part(&self, variable: usize) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if self.native.denominator.degree(variable) > 0 {
            return Err(Error::InvalidInput(
                "partial-fraction polynomial part has a variable-dependent denominator".into(),
            ));
        }
        if self.native.numerator.degree(variable) == u16::MAX {
            return Err(Error::InvalidInput(format!(
                "polynomial exponent overflow while integrating variable `{}`",
                self.ctx.vars()[variable]
            )));
        }
        let numerator = lift_native_polynomial(&self.native.numerator).integrate(variable);
        let denominator = lift_native_polynomial(&self.native.denominator);
        Self::from_native(
            self.ctx.clone(),
            NativeRat::from_num_den(numerator, denominator, &Z, true),
        )
    }

    /// Signed Laurent order at `variable = 0`; `i64::MAX` denotes zero.
    pub fn pole_degree(&self, variable: usize) -> Result<i64> {
        if self.is_zero() {
            return Ok(i64::MAX);
        }
        Ok(self.numerator().min_exponent(variable)? - self.denominator().min_exponent(variable)?)
    }

    /// Leading Laurent coefficient at `variable = 0`.
    pub fn residue(&self, variable: usize) -> Result<Self> {
        if self.is_zero() {
            return Ok(Self::zero(self.ctx().clone()));
        }
        let numerator_degree = self.numerator().min_exponent(variable)?;
        let denominator_degree = self.denominator().min_exponent(variable)?;
        Self::new(
            self.numerator()
                .coefficient_of(variable, numerator_degree)?,
            self.denominator()
                .coefficient_of(variable, denominator_degree)?,
        )
    }
}
