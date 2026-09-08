//! Symbolica-native decomposition of an already factored denominator.

use symbolica::domains::factorized_rational_polynomial::FromNumeratorAndFactorizedDenominator;
use symbolica::domains::rational_polynomial::FromNumeratorAndDenominator;
use symbolica::prelude::*;

use super::FactoredRat;
use crate::core::{NativeRat, Poly, Rat};
use crate::error::{Error, Result};

type IntegerPoly = MultivariatePolynomial<IntegerRing, u16>;
type NativeFactoredRat = FactorizedRationalPolynomial<IntegerRing, u16>;

/// Clear coefficient denominators and split off the exact rational unit.
///
/// Symbolica exposes all primitives needed here (mapped coefficients and
/// polynomial content). Keeping each denominator's unit
/// separate is important: applying one common scale to factors with different
/// powers would change the represented rational function.
pub(super) fn primitive_integer(polynomial: &Poly) -> (IntegerPoly, Rational) {
    debug_assert!(!polynomial.is_zero());
    let (integer, scalar) = polynomial.integer_associate();
    let content = integer.content();
    let mut primitive = integer.div_coeff(&content);
    let mut unit = Q.mul(&scalar, &Rational::from(content));
    if primitive.lcoeff().is_negative() {
        primitive = -primitive;
        unit = -unit;
    }
    (primitive, unit)
}

fn append_or_merge(
    factors: &mut Vec<(IntegerPoly, usize)>,
    base: IntegerPoly,
    exponent: usize,
) -> Result<()> {
    if let Some((_, existing)) = factors.iter_mut().find(|(candidate, _)| *candidate == base) {
        *existing = existing.checked_add(exponent).ok_or_else(|| {
            Error::InvalidInput("native denominator factor exponent overflow".into())
        })?;
    } else {
        factors.push((base, exponent));
    }
    Ok(())
}

fn component_to_rat(
    ctx: std::sync::Arc<crate::core::PolyCtx>,
    component: NativeFactoredRat,
) -> Result<Rat> {
    let numerator = component.numerator.mul_coeff(component.numer_coeff);
    let mut denominator = numerator.constant(component.denom_coeff);
    for (base, exponent) in component.denominators {
        denominator = denominator * &base.pow(exponent);
    }
    Rat::from_native(
        ctx,
        NativeRat::from_num_den(numerator, denominator, &Z, true),
    )
}

impl FactoredRat {
    /// Transfer deferred Q-polynomial bases to Symbolica without expanding
    /// their powers. Native construction owns coefficient-unit normalization.
    pub(crate) fn to_native_factored(&self) -> Result<NativeFactoredRat> {
        let denominators = self
            .den_factors()
            .iter()
            .map(|factor| {
                Ok((
                    factor.base.inner().clone(),
                    usize::try_from(factor.exp).map_err(|_| Error::InvalidExponent(factor.exp))?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(NativeFactoredRat::from_num_den(
            self.numerator().inner().clone(),
            denominators,
            &Z,
            false,
        ))
    }

    /// Split pairwise-coprime target-dependent denominator blocks with
    /// Symbolica's public factorized-rational `apart` implementation.
    ///
    /// Each returned canonical [`Rat`] eagerly materializes one
    /// target-dependent block together with all target-independent factors;
    /// Hyperbolica therefore does not explicitly construct the complete
    /// all-block denominator at this adapter boundary. Symbolica's current
    /// `apart` implementation may still expand powered blocks and intermediate
    /// suffix products internally, so this is a measured blockwise strategy,
    /// not a strict factor-preservation or asymptotic guarantee. `None`
    /// requests the exact full-materialization fallback when user-supplied
    /// blocks overlap. Each component can subsequently be decomposed by
    /// `RationalPolynomial::apart_factored_denominators`.
    pub(crate) fn apart_components(&self, variable: usize) -> Result<Option<Vec<Rat>>> {
        if variable >= self.ctx().len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if self.is_zero() {
            return Ok(Some(vec![Rat::zero(self.ctx().clone())]));
        }

        let (numerator, mut scalar) = primitive_integer(self.numerator());
        let mut dependent = Vec::<(IntegerPoly, usize)>::new();
        let mut independent = Vec::<(IntegerPoly, usize)>::new();

        for factor in self.den_factors() {
            let exponent =
                usize::try_from(factor.exp).map_err(|_| Error::InvalidExponent(factor.exp))?;
            let (base, base_unit) = primitive_integer(&factor.base);
            scalar = Q.div(&scalar, &Q.pow(&base_unit, exponent as u64));

            if base.is_constant() {
                let unit = Rational::from(base.get_constant());
                scalar = Q.div(&scalar, &Q.pow(&unit, exponent as u64));
            } else if base.degree(variable) == 0 {
                append_or_merge(&mut independent, base, exponent)?;
            } else {
                append_or_merge(&mut dependent, base, exponent)?;
            }
        }

        // Symbolica's factorized-denominator algorithm requires distinct
        // blocks to be coprime. Proportional blocks were merged above; a more
        // general overlap is delegated to the canonical RationalPolynomial
        // path instead of risking a failed Diophantine solve.
        for left in 0..dependent.len() {
            for right in left + 1..dependent.len() {
                if !dependent[left].0.gcd(&dependent[right].0).is_one() {
                    return Ok(None);
                }
            }
        }

        let native = NativeFactoredRat {
            numerator,
            numer_coeff: scalar.numerator_ref().clone(),
            denom_coeff: scalar.denominator_ref().clone(),
            denominators: dependent,
        };
        // The current public native implementation indexes its first block
        // after only special-casing one block. A coefficient with no
        // target-dependent denominator needs no Diophantine decomposition.
        let mut components = if native.denominators.is_empty() {
            vec![native]
        } else {
            native.apart(variable)
        };
        for component in &mut components {
            for (base, exponent) in &independent {
                append_or_merge(&mut component.denominators, base.clone(), *exponent)?;
            }
        }

        components
            .into_iter()
            .map(|component| component_to_rat(self.ctx().clone(), component))
            .collect::<Result<Vec<_>>>()
            .map(Some)
    }
}
