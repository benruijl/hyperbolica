use crate::core::{Poly, Rat};
use crate::error::{Error, Result};

/// Truncated Laurent expansion of `function` at `variable = 0`.
///
/// Coefficients are obtained by the exact Cauchy-product recurrence used by
/// HyperFLINT; no floating-point approximation is involved.
pub fn series_expansion(function: &Rat, variable: usize, max_order: i64) -> Result<Rat> {
    let ctx = function.ctx().clone();
    if function.is_zero() {
        return Ok(Rat::zero(ctx));
    }

    let numerator_min = function.numerator().min_exponent(variable)?;
    let denominator_min = function.denominator().min_exponent(variable)?;
    let pole_degree = numerator_min - denominator_min;
    if pole_degree > max_order {
        return Ok(Rat::zero(ctx));
    }

    let coefficient_count = max_order - pole_degree;
    let mut numerator_coefficients = Vec::with_capacity((coefficient_count + 1) as usize);
    let mut denominator_coefficients = Vec::with_capacity((coefficient_count + 1) as usize);
    for offset in 0..=coefficient_count {
        numerator_coefficients.push(Rat::from_poly(
            function
                .numerator()
                .coefficient_of(variable, numerator_min + offset)?,
        ));
        denominator_coefficients.push(Rat::from_poly(
            function
                .denominator()
                .coefficient_of(variable, denominator_min + offset)?,
        ));
    }

    let leading_denominator = &denominator_coefficients[0];
    let mut coefficients = Vec::with_capacity((coefficient_count + 1) as usize);
    for order in 0..=coefficient_count {
        let mut accumulator = numerator_coefficients[order as usize].clone();
        for inner in 1..=order {
            let product = denominator_coefficients[inner as usize]
                .try_mul(&coefficients[(order - inner) as usize])?;
            accumulator = accumulator.try_sub(&product)?;
        }
        coefficients.push(accumulator.try_div(leading_denominator)?);
    }

    let variable_rat = Rat::from_poly(Poly::generator(ctx.clone(), variable)?);
    let mut result = Rat::zero(ctx);
    for (offset, coefficient) in coefficients.into_iter().enumerate() {
        if coefficient.is_zero() {
            continue;
        }
        let exponent = pole_degree + offset as i64;
        let term = coefficient.try_mul(&variable_rat.pow(exponent)?)?;
        result = result.try_add(&term)?;
    }
    Ok(result)
}

/// Substitute `variable -> 1/variable` without introducing negative
/// polynomial exponents.
pub fn substitute_variable_reciprocal(function: &Rat, variable: usize) -> Result<Rat> {
    let ctx = function.ctx().clone();
    let maximum_degree = function
        .numerator()
        .degree(variable)?
        .max(function.denominator().degree(variable)?)
        .max(0);
    let generator = Poly::generator(ctx.clone(), variable)?;

    let reverse = |polynomial: &Poly| -> Result<Poly> {
        if polynomial.is_zero() {
            return Ok(Poly::zero(ctx.clone()));
        }
        let mut output = Poly::zero(ctx.clone());
        let minimum = polynomial.min_exponent(variable)?;
        let maximum = polynomial.degree(variable)?;
        for exponent in minimum..=maximum {
            let coefficient = polynomial.coefficient_of(variable, exponent)?;
            if coefficient.is_zero() {
                continue;
            }
            let reversed_exponent = maximum_degree - exponent;
            if reversed_exponent < 0 {
                return Err(Error::InvalidExponent(reversed_exponent));
            }
            let monomial = generator.pow(reversed_exponent as usize);
            output = output.try_add(&coefficient.try_mul(&monomial)?)?;
        }
        Ok(output)
    };

    Rat::new(
        reverse(function.numerator())?,
        reverse(function.denominator())?,
    )
}

/// Coefficient of `variable^0` in a Laurent expansion at zero.
pub fn coefficient_at_zero(function: &Rat, variable: usize) -> Result<Rat> {
    let ctx = function.ctx().clone();
    if function.is_zero() {
        return Ok(Rat::zero(ctx));
    }

    let denominator_at_zero = function.denominator().coefficient_of(variable, 0)?;
    if !denominator_at_zero.is_zero() {
        return Rat::new(
            function.numerator().coefficient_of(variable, 0)?,
            denominator_at_zero,
        );
    }

    let minimum = function.denominator().min_exponent(variable)?;
    let maximum = function.denominator().degree(variable)?;
    if minimum != maximum {
        return Err(Error::InvalidInput(
            "denominator vanishes at zero but is not a monomial in the expansion variable".into(),
        ));
    }
    Rat::new(
        function.numerator().coefficient_of(variable, minimum)?,
        function.denominator().coefficient_of(variable, minimum)?,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::core::PolyCtx;

    fn context() -> Arc<PolyCtx> {
        PolyCtx::new(["x", "y"]).unwrap()
    }

    #[test]
    fn expands_a_rational_function_exactly() {
        let ctx = context();
        let function = Rat::parse(ctx.clone(), "1/(1-x)").unwrap();
        assert_eq!(
            series_expansion(&function, 0, 3).unwrap(),
            Rat::parse(ctx, "1+x+x^2+x^3").unwrap()
        );
    }

    #[test]
    fn preserves_negative_laurent_orders() {
        let ctx = context();
        let function = Rat::parse(ctx.clone(), "(1+x)/(x^2*(1-x))").unwrap();
        assert_eq!(
            series_expansion(&function, 0, 1).unwrap(),
            Rat::parse(ctx, "x^-2+2*x^-1+2+2*x").unwrap()
        );
    }

    #[test]
    fn reciprocal_substitution_is_exact() {
        let ctx = context();
        let function = Rat::parse(ctx.clone(), "(x+y)/(x^2+1)").unwrap();
        assert_eq!(
            substitute_variable_reciprocal(&function, 0).unwrap(),
            Rat::parse(ctx, "(x+y*x^2)/(1+x^2)").unwrap()
        );
    }
}
