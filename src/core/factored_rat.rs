use std::ops::{Add, Div, Mul, Neg, Sub};
use std::sync::Arc;

use super::{Poly, PolyCtx, Rat};
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

    fn require_poly_context(&self, polynomial: &Poly) -> Result<()> {
        if self.ctx().vars() == polynomial.ctx().vars() {
            Ok(())
        } else {
            Err(Error::ContextMismatch)
        }
    }

    fn require_same_context(&self, other: &Self) -> Result<()> {
        if self.ctx().vars() == other.ctx().vars() {
            Ok(())
        } else {
            Err(Error::ContextMismatch)
        }
    }
}

fn exponent_as_usize(exponent: i64) -> Result<usize> {
    usize::try_from(exponent).map_err(|_| Error::InvalidExponent(exponent))
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

impl Add<&FactoredRat> for &FactoredRat {
    type Output = FactoredRat;

    fn add(self, rhs: &FactoredRat) -> Self::Output {
        self.try_add(rhs)
            .expect("factored-rational addition failed")
    }
}

impl Sub<&FactoredRat> for &FactoredRat {
    type Output = FactoredRat;

    fn sub(self, rhs: &FactoredRat) -> Self::Output {
        self.try_sub(rhs)
            .expect("factored-rational subtraction failed")
    }
}

impl Mul<&FactoredRat> for &FactoredRat {
    type Output = FactoredRat;

    fn mul(self, rhs: &FactoredRat) -> Self::Output {
        self.try_mul(rhs)
            .expect("factored-rational multiplication failed")
    }
}

impl Div<&FactoredRat> for &FactoredRat {
    type Output = FactoredRat;

    fn div(self, rhs: &FactoredRat) -> Self::Output {
        self.try_div(rhs)
            .expect("factored-rational division failed")
    }
}

impl Neg for &FactoredRat {
    type Output = FactoredRat;

    fn neg(self) -> Self::Output {
        self.negated()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Arc<PolyCtx> {
        PolyCtx::new(["x", "y", "z"]).unwrap()
    }

    fn value_equal(left: &Rat, right: &Rat) -> bool {
        left.numerator()
            .try_mul(right.denominator())
            .unwrap()
            .equal(&right.numerator().try_mul(left.denominator()).unwrap())
    }

    fn pseudo_random_poly(ctx: Arc<PolyCtx>, seed: u64) -> Poly {
        let x = Poly::generator(ctx.clone(), 0).unwrap();
        let y = Poly::generator(ctx.clone(), 1).unwrap();
        let z = Poly::generator(ctx.clone(), 2).unwrap();
        let mut result = Poly::from_int(ctx.clone(), 1 + (seed % 5) as i64);
        if seed & 1 != 0 {
            result = result.try_add(&x).unwrap();
        }
        if seed & 2 != 0 {
            result = result
                .try_add(&y.try_mul(&Poly::from_int(ctx.clone(), 2)).unwrap())
                .unwrap();
        }
        if seed & 4 != 0 {
            result = result.try_add(&z).unwrap();
        }
        if seed & 8 != 0 {
            result = result.try_add(&x.try_mul(&y).unwrap()).unwrap();
        }
        result
    }

    #[test]
    fn parse_defers_denominator_power_and_preserves_value() {
        let ctx = context();
        for expression in [
            "(x + y)^2/(1 + x)^3",
            "(x + y)/(1 + x)",
            "x + y",
            "1/(1 + x)^4",
            "1/2*x + y",
            "x/((x + y)*(1 + x))",
        ] {
            let factored = FactoredRat::parse(ctx.clone(), expression).unwrap();
            let ordinary = Rat::parse(ctx.clone(), expression).unwrap();
            assert!(value_equal(&factored.materialize().unwrap(), &ordinary));
        }

        let factored = FactoredRat::parse(ctx, "(x + y)^2/(1 + x)^3").unwrap();
        assert_eq!(factored.den_factors().len(), 1);
        assert_eq!(factored.den_factors()[0].exp, 3);
        assert_eq!(factored.den_factors()[0].base.n_terms(), 2);
    }

    #[test]
    fn factors_merge_and_known_powers_peel_exactly() {
        let ctx = context();
        let base = Poly::parse(ctx.clone(), "x+y+1").unwrap();
        let other = Poly::parse(ctx.clone(), "y+3").unwrap();
        let numerator = base
            .pow(2)
            .try_mul(&Poly::parse(ctx.clone(), "x+2").unwrap())
            .unwrap();
        let mut factored = FactoredRat::from_poly(numerator);
        factored.push_factor(&base, 2).unwrap();
        factored.push_factor(&base, 3).unwrap();
        factored.push_factor(&other, 1).unwrap();
        assert_eq!(factored.den_factors().len(), 2);
        assert_eq!(factored.den_factors()[0].exp, 5);

        let before = factored.materialize().unwrap();
        assert_eq!(factored.peel_known_factors(1).unwrap(), 2);
        assert_eq!(factored.den_factors()[0].exp, 3);
        assert!(!base.divides(factored.numerator()).unwrap());
        assert!(value_equal(&factored.materialize().unwrap(), &before));
    }

    #[test]
    fn arithmetic_and_derivative_match_materialized_oracle() {
        let ctx = context();
        let left = Rat::parse(ctx.clone(), "(x+z)/(x+1)^2").unwrap();
        let right = Rat::parse(ctx.clone(), "(y+2)/(x*y+1)").unwrap();
        let factored_left = FactoredRat::from_rat(&left);
        let factored_right = FactoredRat::from_rat(&right);

        let cases = [
            (
                factored_left.try_add(&factored_right).unwrap(),
                left.try_add(&right).unwrap(),
            ),
            (
                factored_left.try_sub(&factored_right).unwrap(),
                left.try_sub(&right).unwrap(),
            ),
            (
                factored_left.try_mul(&factored_right).unwrap(),
                left.try_mul(&right).unwrap(),
            ),
            (
                factored_left.try_div(&factored_right).unwrap(),
                left.try_div(&right).unwrap(),
            ),
            (factored_left.pow(-3).unwrap(), left.pow(-3).unwrap()),
            (
                factored_left.derivative(0).unwrap(),
                left.derivative(0).unwrap(),
            ),
        ];
        for (factored, oracle) in cases {
            assert!(value_equal(&factored.materialize().unwrap(), &oracle));
        }

        let independent = FactoredRat::parse(ctx, "(x^2+z)/(y+1)^3").unwrap();
        let derivative = independent.derivative(0).unwrap();
        assert_eq!(derivative.den_factors()[0].exp, 3);
    }

    #[test]
    fn deterministic_randomized_operations_are_value_equivalent() {
        let ctx = context();
        for seed in 1..48 {
            let left = Rat::new(
                pseudo_random_poly(ctx.clone(), seed),
                pseudo_random_poly(ctx.clone(), seed + 7),
            )
            .unwrap();
            let right = Rat::new(
                pseudo_random_poly(ctx.clone(), seed + 13),
                pseudo_random_poly(ctx.clone(), seed + 23),
            )
            .unwrap();
            let factored_left = FactoredRat::from_rat(&left);
            let factored_right = FactoredRat::from_rat(&right);

            let comparisons = [
                (
                    factored_left.try_mul(&factored_right).unwrap(),
                    left.try_mul(&right).unwrap(),
                ),
                (
                    factored_left.try_add(&factored_right).unwrap(),
                    left.try_add(&right).unwrap(),
                ),
                (
                    factored_left.try_sub(&factored_right).unwrap(),
                    left.try_sub(&right).unwrap(),
                ),
                (
                    factored_left.try_div(&factored_right).unwrap(),
                    left.try_div(&right).unwrap(),
                ),
                (factored_left.pow(3).unwrap(), left.pow(3).unwrap()),
                (
                    factored_left.derivative((seed % 3) as usize).unwrap(),
                    left.derivative((seed % 3) as usize).unwrap(),
                ),
            ];
            for (factored, oracle) in comparisons {
                assert!(value_equal(&factored.materialize().unwrap(), &oracle));
            }

            let chain = factored_left
                .try_mul(&factored_right)
                .unwrap()
                .try_add(&factored_left)
                .unwrap()
                .try_div(&factored_right)
                .unwrap();
            let oracle = left
                .try_mul(&right)
                .unwrap()
                .try_add(&left)
                .unwrap()
                .try_div(&right)
                .unwrap();
            assert!(value_equal(&chain.materialize().unwrap(), &oracle));
        }
    }
}
