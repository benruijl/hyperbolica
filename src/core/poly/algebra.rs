use symbolica::domains::InternalOrdering;
use symbolica::prelude::*;

use super::{Factored, Poly, SymbolicaPoly};
use crate::error::{Error, Result};

impl Poly {
    fn require_same_context(&self, other: &Self) -> Result<()> {
        if self.ctx.is_compatible_with(&other.ctx) {
            Ok(())
        } else {
            Err(Error::ContextMismatch)
        }
    }

    pub fn try_add(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        Ok(Self::from_inner(
            self.ctx.clone(),
            &self.inner + &other.inner,
        ))
    }

    pub fn try_sub(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        Ok(Self::from_inner(
            self.ctx.clone(),
            &self.inner - &other.inner,
        ))
    }

    pub fn try_mul(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        Ok(Self::from_inner(
            self.ctx.clone(),
            &self.inner * &other.inner,
        ))
    }

    pub fn pow(&self, exponent: usize) -> Self {
        Self::from_inner(self.ctx.clone(), self.inner.pow(exponent))
    }

    pub fn derivative(&self, variable: usize) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        Ok(Self::from_inner(
            self.ctx.clone(),
            self.inner.derivative(variable),
        ))
    }

    /// Substitute one variable by an exact rational number.
    pub fn substitute_rational(&self, variable: usize, value: &Rational) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        Ok(Self::from_inner(
            self.ctx.clone(),
            self.inner.replace(variable, value),
        ))
    }

    /// Substitute one variable by an exact integer.
    pub fn substitute_integer(&self, variable: usize, value: &Integer) -> Result<Self> {
        self.substitute_rational(variable, &Rational::from(value))
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
        Ok(self.inner.replace_all(values))
    }

    /// Evaluate at a complete exact integer point.
    pub fn evaluate_integer(&self, values: &[Integer]) -> Result<Rational> {
        let values = values.iter().map(Rational::from).collect::<Vec<_>>();
        self.evaluate_rational(&values)
    }

    /// Integrate with respect to one variable, choosing zero integration
    /// constant. Symbolica performs the coefficient divisions in `Q`.
    pub fn integrate(&self, variable: usize) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if self.degree(variable)? == i64::from(u16::MAX) {
            return Err(Error::InvalidInput(format!(
                "polynomial exponent overflow while integrating variable `{}`",
                self.ctx.vars()[variable]
            )));
        }
        Ok(Self::from_inner(
            self.ctx.clone(),
            self.inner.integrate(variable),
        ))
    }

    pub fn div_exact(&self, divisor: &Self) -> Result<Self> {
        self.require_same_context(divisor)?;
        if divisor.is_zero() {
            return Err(Error::DivisionByZero);
        }
        self.inner
            .try_div_exact(&divisor.inner)
            .map(|inner| Self::from_inner(self.ctx.clone(), inner))
            .ok_or(Error::InexactDivision)
    }

    pub fn divides(&self, dividend: &Self) -> Result<bool> {
        self.require_same_context(dividend)?;
        if self.is_zero() {
            return Ok(dividend.is_zero());
        }
        Ok(dividend.inner.try_div_exact(&self.inner).is_some())
    }

    pub fn div_rem(&self, divisor: &Self) -> Result<(Self, Self)> {
        self.require_same_context(divisor)?;
        if divisor.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let (quotient, remainder) = self.inner.quot_rem(&divisor.inner, false);
        Ok((
            Self::from_inner(self.ctx.clone(), quotient),
            Self::from_inner(self.ctx.clone(), remainder),
        ))
    }

    pub fn gcd(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        Ok(Self::from_inner(
            self.ctx.clone(),
            self.inner.gcd(&other.inner),
        ))
    }

    pub fn resultant(&self, other: &Self, variable: usize) -> Result<Self> {
        self.require_same_context(other)?;
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        let left = self.inner.to_univariate(variable);
        let right = other.inner.to_univariate(variable);
        Ok(Self::from_inner(self.ctx.clone(), left.resultant(&right)))
    }

    /// HyperFLINT's historical `Res(p, p') / lc(p)` convention.
    ///
    /// For even degrees this differs by a sign from the standard
    /// discriminant. The JSON compatibility bridge deliberately exposes this
    /// legacy operation; new Rust callers should use [`Self::discriminant`].
    pub fn resultant_discriminant(&self, variable: usize) -> Result<Self> {
        if self.degree(variable)? < 1 {
            return Ok(Self::one(self.ctx.clone()));
        }
        let derivative = self.derivative(variable)?;
        let resultant = self.resultant(&derivative, variable)?;
        let leading = Self::from_inner(self.ctx.clone(), self.inner.univariate_lcoeff(variable));
        resultant.div_exact(&leading)
    }

    /// Standard polynomial discriminant
    /// `(-1)^(n(n-1)/2) Res(p,p') / lc(p)`.
    pub fn discriminant(&self, variable: usize) -> Result<Self> {
        let degree = self.degree(variable)?;
        let legacy = self.resultant_discriminant(variable)?;
        if degree >= 1 && (degree * (degree - 1) / 2) % 2 != 0 {
            Ok(-&legacy)
        } else {
            Ok(legacy)
        }
    }

    pub fn degree(&self, variable: usize) -> Result<i64> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if self.is_zero() {
            Ok(-1)
        } else {
            Ok(i64::from(self.inner.degree(variable)))
        }
    }

    pub fn min_exponent(&self, variable: usize) -> Result<i64> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if self.is_zero() {
            Ok(i64::MAX)
        } else {
            Ok(i64::from(self.inner.degree_bounds(variable).0))
        }
    }

    pub fn coefficient_of(&self, variable: usize, exponent: i64) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if !(0..=u16::MAX as i64).contains(&exponent) {
            return Ok(Self::zero(self.ctx.clone()));
        }
        let exponent = exponent as u16;
        let mut inner = SymbolicaPoly::new(&Q, None, self.ctx.variable_map());
        for (term, coefficient) in self
            .inner
            .exponents_iter()
            .zip(self.inner.coefficients.iter())
        {
            if term[variable] != exponent {
                continue;
            }
            let mut output_exponents = term.to_vec();
            output_exponents[variable] = 0;
            inner.append_monomial(coefficient.clone(), &output_exponents);
        }
        Ok(Self::from_inner(self.ctx.clone(), inner))
    }

    pub fn scalar_div(&self, scalar: &Self) -> Result<Self> {
        self.require_same_context(scalar)?;
        if !scalar.is_rational_constant() {
            return Err(Error::InvalidInput("scalar divisor is not constant".into()));
        }
        let coefficient = scalar.inner.get_constant();
        if Q.is_zero(&coefficient) {
            return Err(Error::DivisionByZero);
        }
        Ok(Self::from_inner(
            self.ctx.clone(),
            self.inner.clone().div_coeff(&coefficient),
        ))
    }

    pub fn canonical_proportional_form(&self) -> Self {
        if self.is_zero() {
            return Self::zero(self.ctx.clone());
        }
        let leading = self.inner.lcoeff();
        Self::from_inner(self.ctx.clone(), self.inner.clone().div_coeff(&leading))
    }

    pub fn factor(&self) -> Factored {
        if self.is_zero() {
            return Factored {
                constant: Q.zero(),
                factors: Vec::new(),
            };
        }

        let mut constant = Q.one();
        let mut factors = Vec::new();
        for (factor, exponent) in self.inner.factor() {
            if factor.is_constant() {
                constant = Q.mul(&constant, &Q.pow(&factor.get_constant(), exponent as u64));
            } else {
                factors.push((Self::from_inner(self.ctx.clone(), factor), exponent));
            }
        }
        factors.sort_by(|(left, _), (right, _)| {
            left.total_degree()
                .cmp(&right.total_degree())
                .then_with(|| left.inner.internal_cmp(&right.inner))
        });
        Factored { constant, factors }
    }
}
