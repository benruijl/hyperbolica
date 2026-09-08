//! HyperFLINT-shaped output from Symbolica's native factored-coefficient PF.

use std::sync::Arc;

use symbolica::domains::factorized_rational_polynomial::FromNumeratorAndFactorizedDenominator;
use symbolica::domains::rational_polynomial::FromNumeratorAndDenominator;
use symbolica::prelude::{Factorize, FactorizedRationalPolynomial, IntegerRing, Z};

use crate::core::{NativeRat, PolyCtx, Rat};
use crate::error::{Error, Result};

use super::{PartialFractionPole, PartialFractionization, canonicalize_poles, merge_pole};

type NativeFactored = FactorizedRationalPolynomial<IntegerRing, u16>;

/// Detect a proper, completely linear denominator. Factorization and all
/// coefficient arithmetic belong to Symbolica; this layer only selects the
/// port's supported shape and maps its native output convention.
pub(super) fn try_linear_partial_fractions(
    function: &Rat,
    variable: usize,
) -> Result<Option<PartialFractionization>> {
    let native = function.native();
    if native.numerator.degree(variable) >= native.denominator.degree(variable) {
        return Ok(None);
    }
    let denominators = native.denominator.factor();
    if denominators
        .iter()
        .any(|(base, _)| base.degree(variable) > 1)
    {
        return Ok(None);
    }
    let factored = NativeFactored::from_num_den(native.numerator.clone(), denominators, &Z, false);
    native_linear_partial_fractions(function.ctx(), &factored, variable).map(Some)
}

pub(super) fn native_linear_partial_fractions(
    ctx: &Arc<PolyCtx>,
    function: &NativeFactored,
    variable: usize,
) -> Result<PartialFractionization> {
    let mut output = PartialFractionization {
        polynomial_part: Rat::zero(ctx.clone()),
        poles: Vec::new(),
    };
    if function.is_zero() {
        return Ok(output);
    }
    for (coefficient, base, exponent) in function.apart_factored_denominators(variable) {
        if coefficient.is_zero() {
            continue;
        }
        if exponent == 0 {
            return Err(Error::InvalidInput(
                "Symbolica returned a zero partial-fraction exponent".into(),
            ));
        }
        // Proper native linear terms are over monic x-pole blocks. Only the
        // final coefficient is materialized; no leading-coefficient power is
        // expanded and then divided out again at this boundary.
        let base = Rat::from_native(ctx.clone(), base.to_rational_polynomial())?;
        if base.native().numerator.degree(variable) != 1
            || base.native().denominator.degree(variable) != 0
        {
            return Err(Error::InvalidInput(
                "Symbolica returned a non-linear base for a proper linear input".into(),
            ));
        }
        let univariate = base.native().numerator.to_univariate(variable);
        let leading = univariate.coefficients()[1].clone();
        // Both polynomials have already been remapped to this context, so
        // monicity is exact structural equality, not another rational GCD.
        if leading != base.native().denominator {
            return Err(Error::InvalidInput(
                "Symbolica returned a non-monic proper linear partial-fraction base".into(),
            ));
        }
        let pole = Rat::from_native(
            ctx.clone(),
            NativeRat::from_num_den(-univariate.coefficients()[0].clone(), leading, &Z, true),
        )?;
        let coefficient = Rat::from_native(ctx.clone(), coefficient.to_rational_polynomial())?;
        if coefficient.native().numerator.degree(variable) > 0
            || coefficient.native().denominator.degree(variable) > 0
        {
            return Err(Error::InvalidInput(
                "Symbolica returned a partial-fraction coefficient depending on the target".into(),
            ));
        }
        let mut coefs = vec![Rat::zero(ctx.clone()); exponent];
        coefs[exponent - 1] = coefficient;
        merge_pole(
            &mut output,
            PartialFractionPole {
                pole,
                multiplicity: exponent,
                coefs,
            },
        )?;
    }
    canonicalize_poles(&mut output);
    Ok(output)
}
