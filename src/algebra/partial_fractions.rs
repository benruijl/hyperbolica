use symbolica::domains::rational_polynomial::FromNumeratorAndDenominator;
use symbolica::prelude::{RationalPolynomialField, Z};

use crate::core::{NativeRat, Poly, Rat};
use crate::error::{Error, Result};

use super::algebraic_introduction::introduce_quadratic;
use super::algebraic_letters::join_algebraic_letter_session;
use super::linear_factors::{LinearFactorOptions, linear_factors_with_options};

mod factored;
mod factored_coefficients;
mod linear;

use linear::try_linear_partial_fractions;

pub use factored::{partial_fractions_factored, partial_fractions_factored_with_options};

/// Policy for a univariate partial-fraction decomposition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PartialFractionOptions<'a> {
    /// Split irreducible quadratics over registered formal root letters.
    pub introduce_algebraic_letters: bool,
    /// Variables scheduled after this one. A quadratic depending on any of
    /// them must remain nonlinear and is therefore rejected by this layer.
    pub forbidden_variables: &'a [usize],
}

/// All coefficients belonging to one distinct pole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialFractionPole {
    pub pole: Rat,
    pub multiplicity: usize,
    /// Coefficients in ascending pole order: `coefs[k - 1]` multiplies
    /// `1 / (x - pole)^k`.
    pub coefs: Vec<Rat>,
}

/// The polynomial part and proper-fraction terms of a rational function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialFractionization {
    pub polynomial_part: Rat,
    /// Distinct poles in canonical HyperFLINT order: multiplicity first, then
    /// the normalized pole's structural order.
    pub poles: Vec<PartialFractionPole>,
}

fn canonicalize_poles(output: &mut PartialFractionization) {
    output.poles.sort_unstable_by(|left, right| {
        left.multiplicity
            .cmp(&right.multiplicity)
            .then_with(|| left.pole.structural_cmp(&right.pole))
    });
}

fn merge_pole(output: &mut PartialFractionization, incoming: PartialFractionPole) -> Result<()> {
    let ctx = output.polynomial_part.ctx().clone();
    let index = output
        .poles
        .iter()
        .position(|candidate| candidate.pole.equal(&incoming.pole));
    let index = match index {
        Some(index) => index,
        None => {
            output.poles.push(PartialFractionPole {
                pole: incoming.pole.clone(),
                multiplicity: 0,
                coefs: Vec::new(),
            });
            output.poles.len() - 1
        }
    };
    let target = &mut output.poles[index];
    if target.coefs.len() < incoming.coefs.len() {
        target
            .coefs
            .resize_with(incoming.coefs.len(), || Rat::zero(ctx.clone()));
    }
    for (position, coefficient) in incoming.coefs.into_iter().enumerate() {
        target.coefs[position] = target.coefs[position].try_add(&coefficient)?;
    }
    target.multiplicity = target.multiplicity.max(incoming.multiplicity);
    Ok(())
}

fn merge_decomposition(
    output: &mut PartialFractionization,
    incoming: PartialFractionization,
) -> Result<()> {
    output.polynomial_part = output.polynomial_part.try_add(&incoming.polynomial_part)?;
    for pole in incoming.poles {
        merge_pole(output, pole)?;
    }
    Ok(())
}

fn nonlinear_denominator_error(base: &Rat, variable: usize, degree: i64) -> Error {
    Error::InvalidInput(format!(
        "partial fractions require linear denominator factors in variable `{}`; \
         factor `{base}` has degree {degree}",
        base.ctx().vars()[variable]
    ))
}

/// Divide in the selected variable over the rational-function coefficient
/// field, returning the polynomial quotient and the proper remainder.
///
/// Multivariate polynomial division is not sufficient here: the leading
/// coefficients may depend on the other variables and therefore need to be
/// inverted. This mirrors the quotient extraction used by Symbolica's native
/// rational-polynomial `apart` implementation.
fn polynomial_part_and_proper_remainder(function: &Rat, variable: usize) -> Result<(Rat, Rat)> {
    if function.numerator_degree(variable)? < function.denominator_degree(variable)? {
        return Ok((Rat::zero(function.ctx().clone()), function.clone()));
    }

    let native = function.native();
    let coefficient_field = RationalPolynomialField::from_poly(&native.numerator);
    let numerator = native.numerator.to_univariate(variable).map_coeff(
        |coefficient| coefficient.clone().into(),
        coefficient_field.clone(),
    );
    let denominator = native
        .denominator
        .to_univariate(variable)
        .map_coeff(|coefficient| coefficient.clone().into(), coefficient_field);
    let (quotient, _) = numerator.quot_rem(&denominator);
    let polynomial_part =
        Rat::from_native(function.ctx().clone(), NativeRat::from_univariate(quotient))?;
    let proper_remainder = function.try_sub(&polynomial_part)?;
    Ok((polynomial_part, proper_remainder))
}

/// Replace every admissible quadratic denominator factor in one operation and
/// decompose the resulting fully split rational function.
///
/// Splitting isolated components of an `apart` result is not equivalent over
/// the independent formal `Wm`/`Wp` indeterminates: coefficients of the other
/// components were computed using the original quadratic and only become
/// equivalent after applying the registered Vieta relations. Constructing the
/// entire split denominator first keeps the returned decomposition exactly
/// valid in the formal-symbol field, without relying on a later reduction.
fn try_algebraic_linear_partial_fractions(
    function: &Rat,
    variable: usize,
    forbidden_variables: &[usize],
) -> Result<Option<PartialFractionization>> {
    let (polynomial_part, proper_remainder) =
        polynomial_part_and_proper_remainder(function, variable)?;
    if proper_remainder.is_zero() {
        return Ok(Some(PartialFractionization {
            polynomial_part,
            poles: Vec::new(),
        }));
    }

    let factorization = linear_factors_with_options(
        proper_remainder.denominator(),
        variable,
        &LinearFactorOptions {
            introduce_algebraic_letters: true,
            forbidden_variables,
        },
    )?;
    if !factorization.nonlinear.is_empty() || factorization.linear.is_empty() {
        return Ok(None);
    }

    let ctx = function.ctx().clone();
    let x = Rat::from_poly(Poly::generator(ctx.clone(), variable)?);
    let mut split_denominator = Rat::from_poly(factorization.constant);
    for factor in factorization.linear {
        let exponent = i64::try_from(factor.multiplicity)
            .map_err(|_| Error::InvalidInput("partial-fraction exponent overflow".into()))?;
        split_denominator = split_denominator.try_mul(&x.try_sub(&factor.pole)?.pow(exponent)?)?;
    }

    let split_function =
        Rat::from_poly(proper_remainder.numerator().clone()).try_div(&split_denominator)?;
    let Some(mut decomposition) = try_linear_partial_fractions(&split_function, variable)? else {
        return Ok(None);
    };
    decomposition.polynomial_part = polynomial_part;
    Ok(Some(decomposition))
}

/// Compute a univariate partial-fraction decomposition with Symbolica-backed
/// exact arithmetic.
///
/// Completely linear denominators use Symbolica's factored-coefficient recurrence.
/// Remaining inputs fall back to Symbolica's factored-denominator `apart`
/// output. Both routes retain multiplicities and keep `coefs` in HyperFLINT's
/// ascending order `c_1, ..., c_m`.
pub fn partial_fractions(function: &Rat, variable: usize) -> Result<PartialFractionization> {
    partial_fractions_with_options(function, variable, &PartialFractionOptions::default())
}

/// Compute a Symbolica-backed decomposition, optionally extending the
/// coefficient alphabet by formal roots of irreducible quadratics.
///
/// Fractions with a completely linear denominator use Symbolica's native
/// factored-coefficient partial fractions. In algebraic
/// mode, an improper input is first divided over the rational-function
/// coefficient field, then every quadratic in the proper remainder is split
/// together. Inputs that remain nonlinear retain Symbolica's native
/// `apart_factored_denominators` fallback.
pub fn partial_fractions_with_options(
    function: &Rat,
    variable: usize,
    options: &PartialFractionOptions<'_>,
) -> Result<PartialFractionization> {
    let _session = options
        .introduce_algebraic_letters
        .then(join_algebraic_letter_session)
        .transpose()?;
    // Validate the variable before any trivial-case return.
    let ctx = function.ctx().clone();
    if variable >= ctx.len() {
        return Err(Error::UnknownVariable(variable.to_string()));
    }
    let denominator_degree = i64::from(function.native().denominator.degree(variable));
    let mut output = PartialFractionization {
        polynomial_part: Rat::zero(ctx.clone()),
        poles: Vec::new(),
    };

    if function.is_zero() {
        return Ok(output);
    }
    if denominator_degree <= 0 {
        output.polynomial_part = function.clone();
        return Ok(output);
    }

    if options.introduce_algebraic_letters {
        if let Some(linear) =
            try_algebraic_linear_partial_fractions(function, variable, options.forbidden_variables)?
        {
            return Ok(linear);
        }
    } else if let Some(linear) = try_linear_partial_fractions(function, variable)? {
        return Ok(linear);
    }

    for (numerator, denominator_base, exponent) in
        function.native().apart_factored_denominators(variable)
    {
        if exponent == 0 {
            return Err(Error::InvalidInput(
                "Symbolica returned a zero partial-fraction exponent".into(),
            ));
        }

        let numerator = Rat::from_native(ctx.clone(), numerator)?;
        let base = Rat::from_native(ctx.clone(), denominator_base)?;
        let base_denominator_degree = i64::from(base.native().denominator.degree(variable));
        let base_numerator_degree = if base.native().numerator.is_zero() {
            -1
        } else {
            i64::from(base.native().numerator.degree(variable))
        };

        // Symbolica represents the quotient with denominator base 1.  Treat
        // any other target-independent base defensively the same way.
        if base_numerator_degree <= 0 && base_denominator_degree <= 0 {
            let exponent = i64::try_from(exponent)
                .map_err(|_| Error::InvalidInput("partial-fraction exponent overflow".into()))?;
            let term = numerator.try_div(&base.pow(exponent)?)?;
            output.polynomial_part = output.polynomial_part.try_add(&term)?;
            continue;
        }

        if base_denominator_degree > 0 {
            return Err(Error::InvalidInput(format!(
                "Symbolica returned denominator base `{base}` whose own denominator depends on `{}`",
                ctx.vars()[variable]
            )));
        }
        if base_numerator_degree == 2 && options.introduce_algebraic_letters {
            let defining_polynomial = base.numerator().clone();
            let Some(introduced) =
                introduce_quadratic(&defining_polynomial, variable, options.forbidden_variables)?
            else {
                let names = options
                    .forbidden_variables
                    .iter()
                    .filter_map(|&index| ctx.vars().get(index))
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(Error::InvalidInput(format!(
                    "quadratic denominator factor `{base}` depends on a remaining integration variable ({names}); refusing algebraic-letter introduction"
                )));
            };

            let leading_numerator = defining_polynomial.coefficient_of(variable, 2)?;
            let leading = Rat::new(leading_numerator, base.denominator().clone())?;
            let x = Rat::from_poly(Poly::generator(ctx.clone(), variable)?);
            let split_base = leading
                .try_mul(&x.try_sub(&introduced.minus)?)?
                .try_mul(&x.try_sub(&introduced.plus)?)?;
            let exponent_i64 = i64::try_from(exponent)
                .map_err(|_| Error::InvalidInput("partial-fraction exponent overflow".into()))?;
            let split_component = numerator.try_div(&split_base.pow(exponent_i64)?)?;
            let split = partial_fractions(&split_component, variable)?;
            merge_decomposition(&mut output, split)?;
            continue;
        }

        if base_numerator_degree != 1 {
            return Err(nonlinear_denominator_error(
                &base,
                variable,
                base_numerator_degree,
            ));
        }

        // base = (c1*x + c0) / d = (c1/d) * (x - pole). Extract
        // c0 and c1 through Symbolica's native univariate projection, keeping
        // the lazy Q-polynomial compatibility views cold on this hot path.
        let base_univariate = base.native().numerator.to_univariate(variable);
        let constant_coefficient = base_univariate
            .coefficients()
            .first()
            .cloned()
            .unwrap_or_else(|| base.native().numerator.zero());
        let leading_coefficient = base_univariate
            .coefficients()
            .get(1)
            .cloned()
            .ok_or_else(|| nonlinear_denominator_error(&base, variable, 0))?;
        let pole = Rat::from_native(
            ctx.clone(),
            NativeRat::from_num_den(-constant_coefficient, leading_coefficient.clone(), &Z, true),
        )?;
        let leading = Rat::from_native(
            ctx.clone(),
            NativeRat::from_num_den(
                leading_coefficient,
                base.native().denominator.clone(),
                &Z,
                true,
            ),
        )?;
        let exponent_i64 = i64::try_from(exponent)
            .map_err(|_| Error::InvalidInput("partial-fraction exponent overflow".into()))?;
        let coefficient = numerator.try_div(&leading.pow(exponent_i64)?)?;

        // Proper numerators over a linear base must be independent of the
        // selected variable.  Checking this here turns any unexpected
        // Symbolica representation change into a clear error instead of a
        // subtly invalid HyperFLINT-shaped result.
        if coefficient.native().numerator.degree(variable) > 0
            || coefficient.native().denominator.degree(variable) > 0
        {
            return Err(Error::InvalidInput(format!(
                "partial-fraction coefficient `{coefficient}` still depends on `{}`",
                ctx.vars()[variable]
            )));
        }

        let mut coefs = Vec::with_capacity(exponent);
        coefs.resize_with(exponent, || Rat::zero(ctx.clone()));
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

#[cfg(test)]
mod tests;
