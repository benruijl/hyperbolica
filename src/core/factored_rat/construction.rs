//! Construction, parsing, and denominator-factor invariants.

use std::sync::Arc;

use symbolica::prelude::{AtomCore, AtomView, Q, Rational, Z};

use super::{Factor, FactoredRat, exponent_as_usize};
use crate::core::{Poly, PolyCtx, Rat};
use crate::error::{Error, Result};

impl FactoredRat {
    pub fn from_poly(numerator: Poly) -> Self {
        Self {
            numerator,
            den_factors: Vec::new(),
        }
    }

    pub fn from_rat(rational: &Rat) -> Self {
        let mut result = Self::from_poly(rational.numerator().clone());
        if !rational.denominator().is_one() {
            result.den_factors.push(Factor {
                base: rational.denominator().clone(),
                exp: 1,
            });
        }
        result
    }

    /// Convert a bare rational Symbolica atom without formatting, reparsing,
    /// or first constructing one expanded denominator polynomial.
    ///
    /// Symbolica's public factorized-rational conversion owns the algebraic
    /// normalization. This adapter only transfers its exact integer scalar,
    /// numerator, denominator blocks, and powers into Hyperbolica's checked
    /// context-bound representation.
    pub(crate) fn from_atom(ctx: Arc<PolyCtx>, atom: AtomView<'_>) -> Result<Self> {
        let native = atom
            .try_to_factorized_rational_polynomial::<_, _, u16>(&Q, &Z, Some(ctx.variable_map()))
            .map_err(|error| Error::InvalidInput(error.to_string()))?;

        let expected_variables = ctx.variable_map();
        if native.numerator.get_vars_ref() != expected_variables.as_ref()
            || native
                .denominators
                .iter()
                .any(|(factor, _)| factor.get_vars_ref() != expected_variables.as_ref())
        {
            return Err(Error::InvalidInput(
                "atom contains an indeterminate outside its factored-rational context".into(),
            ));
        }

        let scalar = Rational::from((native.numer_coeff, native.denom_coeff));
        let numerator = native
            .numerator
            .map_coeff(|coefficient| Q.to_element_numerator(coefficient.clone()), Q)
            .mul_coeff(scalar);
        let mut result = Self::from_poly(Poly::from_inner(ctx.clone(), numerator));
        for (factor, exponent) in native.denominators {
            let exponent = i64::try_from(exponent).map_err(|_| {
                Error::InvalidInput("denominator factor exponent does not fit in i64".into())
            })?;
            let factor =
                factor.map_coeff(|coefficient| Q.to_element_numerator(coefficient.clone()), Q);
            result.push_factor(&Poly::from_inner(ctx.clone(), factor), exponent)?;
        }
        Ok(result)
    }

    /// Parse `(NUMERATOR_BASE)^p/(DENOMINATOR_BASE)^q` without expanding the
    /// denominator power.
    ///
    /// As in upstream HyperFLINT, a side is recognized as a deferred power
    /// only when its base is fully parenthesized. Bare polynomials and fully
    /// parenthesized expressions without a power are represented with
    /// exponent one. A slash between adjacent digits is treated as part of a
    /// rational coefficient instead of as the top-level rational separator.
    pub fn parse(ctx: Arc<PolyCtx>, expression: &str) -> Result<Self> {
        let slash = top_level_rational_slash(expression);
        let (numerator_expression, denominator_expression) = match slash {
            Some(position) => (&expression[..position], &expression[position + 1..]),
            None => (expression, "1"),
        };

        let (numerator_base, numerator_exponent) = split_power_factor(numerator_expression)?;
        let (denominator_base, denominator_exponent) = split_power_factor(denominator_expression)?;

        let mut numerator = Poly::parse(ctx.clone(), numerator_base)?;
        if numerator_exponent != 1 {
            numerator = numerator.pow(exponent_as_usize(numerator_exponent)?);
        }

        let mut result = Self::from_poly(numerator);
        let denominator = Poly::parse(ctx, denominator_base)?;
        result.push_factor(&denominator, denominator_exponent)?;
        Ok(result)
    }

    pub fn ctx(&self) -> &Arc<PolyCtx> {
        self.numerator.ctx()
    }

    pub fn numerator(&self) -> &Poly {
        &self.numerator
    }

    pub fn den_factors(&self) -> &[Factor] {
        &self.den_factors
    }

    pub fn is_zero(&self) -> bool {
        self.numerator.is_zero()
    }

    /// Add a denominator power, merging an equal base already in the list.
    pub fn push_factor(&mut self, base: &Poly, exponent: i64) -> Result<()> {
        if exponent < 0 {
            return Err(Error::InvalidExponent(exponent));
        }
        if exponent == 0 || base.is_one() {
            return Ok(());
        }
        self.require_poly_context(base)?;
        if base.is_zero() {
            return Err(Error::DivisionByZero);
        }

        if let Some(existing) = self
            .den_factors
            .iter_mut()
            .find(|factor| factor.base.equal(base))
        {
            existing.exp = existing.exp.checked_add(exponent).ok_or_else(|| {
                Error::InvalidInput("denominator factor exponent overflow".into())
            })?;
        } else {
            self.den_factors.push(Factor {
                base: base.clone(),
                exp: exponent,
            });
        }
        Ok(())
    }

    fn require_poly_context(&self, polynomial: &Poly) -> Result<()> {
        if self.ctx().is_compatible_with(polynomial.ctx()) {
            Ok(())
        } else {
            Err(Error::ContextMismatch)
        }
    }

    pub(super) fn require_same_context(&self, other: &Self) -> Result<()> {
        if self.ctx().is_compatible_with(other.ctx()) {
            Ok(())
        } else {
            Err(Error::ContextMismatch)
        }
    }
}

fn top_level_rational_slash(expression: &str) -> Option<usize> {
    let bytes = expression.as_bytes();
    let mut depth = 0_i64;
    for (position, &byte) in bytes.iter().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b'/' if depth == 0 => {
                let coefficient_slash = position > 0
                    && position + 1 < bytes.len()
                    && bytes[position - 1].is_ascii_digit()
                    && bytes[position + 1].is_ascii_digit();
                if !coefficient_slash {
                    return Some(position);
                }
            }
            _ => {}
        }
    }
    None
}

/// Return the base and exponent for a clean `(base)^digits` side.
fn split_power_factor(side: &str) -> Result<(&str, i64)> {
    let side = side.trim();
    if !side.starts_with('(') {
        return Ok((side, 1));
    }

    let mut depth = 0_i64;
    let mut closing = None;
    for (position, byte) in side.bytes().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    closing = Some(position);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(closing) = closing else {
        return Ok((side, 1));
    };

    let base = &side[1..closing];
    let rest = side[closing + 1..].trim();
    if rest.is_empty() {
        return Ok((base, 1));
    }
    let Some(exponent) = rest.strip_prefix('^') else {
        return Ok((side, 1));
    };
    if exponent.is_empty() || !exponent.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok((side, 1));
    }
    let exponent = exponent.parse::<i64>().map_err(|_| {
        Error::InvalidInput(format!("factor exponent `{exponent}` does not fit in i64"))
    })?;
    Ok((base, exponent))
}
