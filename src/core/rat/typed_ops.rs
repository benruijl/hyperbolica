//! Typed evaluation, substitution, integration, and Laurent operations.

use symbolica::domains::rational::RationalField;
use symbolica::domains::rational_polynomial::{
    FromNumeratorAndDenominator, RationalPolynomialField,
};
use symbolica::prelude::*;

use super::{NativeRat, Rat};
use crate::error::{Error, Result};

type RationalPoly = MultivariatePolynomial<RationalField, u16>;

fn lift_native_polynomial(polynomial: &MultivariatePolynomial<IntegerRing, u16>) -> RationalPoly {
    polynomial.map_coeff(|coefficient| Q.to_element_numerator(coefficient.clone()), Q)
}

fn native_polynomial_coefficient(
    polynomial: &MultivariatePolynomial<IntegerRing, u16>,
    variable: usize,
    exponent: u16,
) -> MultivariatePolynomial<IntegerRing, u16> {
    let mut coefficient = polynomial.zero();
    let mut powers = vec![0; polynomial.nvars()];
    for (term, value) in polynomial.exponents_iter().zip(&polynomial.coefficients) {
        if term[variable] == exponent {
            powers.copy_from_slice(term);
            powers[variable] = 0;
            coefficient.append_monomial_back(value.clone(), &powers);
        }
    }
    coefficient
}

fn evaluate_native_polynomial_at_rat(
    polynomial: &MultivariatePolynomial<IntegerRing, u16>,
    variable: usize,
    replacement: &NativeRat,
) -> NativeRat {
    let rational_function_field = RationalPolynomialField::<IntegerRing, u16>::new(Z);
    polynomial
        .to_univariate(variable)
        .map_coeff(
            |coefficient| {
                NativeRat::from_num_den(coefficient.clone(), coefficient.one(), &Z, false)
            },
            rational_function_field,
        )
        .evaluate(replacement)
}

impl Rat {
    /// Substitute one variable by another exact rational function.
    ///
    /// Symbolica exposes every required primitive publicly: split each native
    /// integer polynomial with `to_univariate`, lift its coefficient ring to
    /// `RationalPolynomialField`, and use the univariate Horner `evaluate`.
    /// This keeps the canonical native representation throughout and avoids
    /// materializing the lazy `Poly<Q>` compatibility views.
    pub fn substitute_rat(&self, variable: usize, replacement: &Self) -> Result<Self> {
        self.require_same_context(replacement)?;
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if !self.depends_on(variable)? {
            return Ok(self.clone());
        }
        if let Some(value) = replacement.rational_constant() {
            return self.substitute_rational(variable, &value);
        }

        let numerator = evaluate_native_polynomial_at_rat(
            &self.native.numerator,
            variable,
            replacement.native(),
        );
        let denominator = evaluate_native_polynomial_at_rat(
            &self.native.denominator,
            variable,
            replacement.native(),
        );
        if denominator.is_zero() {
            return Err(Error::DivisionByZero);
        }
        Self::from_native(self.ctx.clone(), &numerator / &denominator)
    }

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
        if !self.depends_on(variable)? {
            return Ok(self.clone());
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
        if !self.depends_on(variable)? {
            return Ok(self.clone());
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

    /// Highest numerator exponent of `variable`; zero has degree `-1`.
    ///
    /// This queries Symbolica's canonical integer polynomial directly and
    /// does not materialize the lazy `Poly<Q>` compatibility view.
    pub fn numerator_degree(&self, variable: usize) -> Result<i64> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if self.native.numerator.is_zero() {
            Ok(-1)
        } else {
            Ok(i64::from(self.native.numerator.degree(variable)))
        }
    }

    /// Highest denominator exponent of `variable`.
    ///
    /// This queries Symbolica's canonical integer polynomial directly and
    /// does not materialize the lazy `Poly<Q>` compatibility view.
    pub fn denominator_degree(&self, variable: usize) -> Result<i64> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        Ok(i64::from(self.native.denominator.degree(variable)))
    }

    /// Whether either canonical polynomial contains `variable`.
    ///
    /// Symbolica's native `contains` scan avoids allocating the public
    /// compatibility numerator and denominator solely for a dependency
    /// guard.
    pub fn depends_on(&self, variable: usize) -> Result<bool> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        Ok(self.native.numerator.contains(variable) || self.native.denominator.contains(variable))
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
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if self.is_zero() {
            return Ok(i64::MAX);
        }
        let numerator_min = i64::from(self.native.numerator.degree_bounds(variable).0);
        let denominator_min = i64::from(self.native.denominator.degree_bounds(variable).0);
        Ok(numerator_min - denominator_min)
    }

    /// Leading Laurent coefficient at `variable = 0`.
    pub fn residue(&self, variable: usize) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if self.is_zero() {
            return Ok(self.clone());
        }
        let numerator_degree = self.native.numerator.degree_bounds(variable).0;
        let denominator_degree = self.native.denominator.degree_bounds(variable).0;
        Self::from_native(
            self.ctx.clone(),
            NativeRat::from_num_den(
                native_polynomial_coefficient(&self.native.numerator, variable, numerator_degree),
                native_polynomial_coefficient(
                    &self.native.denominator,
                    variable,
                    denominator_degree,
                ),
                &Z,
                true,
            ),
        )
    }
}
