use symbolica::domains::rational_polynomial::FromNumeratorAndDenominator;
use symbolica::prelude::{RationalPolynomialField, Z};

use crate::core::{NativeRat, Poly, Rat};
use crate::error::{Error, Result};

use super::algebraic_introduction::introduce_quadratic;
use super::algebraic_letters::join_algebraic_letter_session;
use super::linear_factors::{
    LinearFactor, LinearFactorOptions, linear_factors, linear_factors_with_options,
};

mod factored;

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

fn add_rats(left: &Rat, right: &Rat) -> Result<Rat> {
    if left.is_zero() {
        Ok(right.clone())
    } else if right.is_zero() {
        Ok(left.clone())
    } else {
        left.try_add(right)
    }
}

fn sub_rats(left: &Rat, right: &Rat) -> Result<Rat> {
    if right.is_zero() {
        Ok(left.clone())
    } else if left.is_zero() {
        Ok(right.negated())
    } else {
        left.try_sub(right)
    }
}

fn mul_rats(left: &Rat, right: &Rat) -> Result<Rat> {
    if left.is_zero() || right.is_zero() {
        Ok(Rat::zero(left.ctx().clone()))
    } else if left.is_one() {
        Ok(right.clone())
    } else if right.is_one() {
        Ok(left.clone())
    } else {
        left.try_mul(right)
    }
}

/// Multiply a truncated power series by `t + constant` in place.
fn multiply_by_shifted_linear(
    coefficients: &mut [Rat],
    constant: &Rat,
    degree: usize,
) -> Result<usize> {
    if coefficients.is_empty() {
        return Ok(0);
    }

    let new_degree = degree.saturating_add(1).min(coefficients.len() - 1);
    for exponent in (1..=new_degree).rev() {
        let scaled = if exponent <= degree {
            mul_rats(&coefficients[exponent], constant)?
        } else {
            Rat::zero(constant.ctx().clone())
        };
        coefficients[exponent] = add_rats(&scaled, &coefficients[exponent - 1])?;
    }
    coefficients[0] = mul_rats(&coefficients[0], constant)?;
    Ok(new_degree)
}

/// Return the first `terms` coefficients of `polynomial(pole + t)`.
fn shifted_polynomial_series(
    polynomial: &Poly,
    variable: usize,
    pole: &Rat,
    terms: usize,
) -> Result<Vec<Rat>> {
    let mut shifted = vec![Rat::zero(polynomial.ctx().clone()); terms];
    if terms == 0 || polynomial.is_zero() {
        return Ok(shifted);
    }

    let degree = usize::try_from(polynomial.degree(variable)?)
        .map_err(|_| Error::InvalidInput("negative polynomial degree".into()))?;
    let mut shifted_degree = 0;
    for exponent in (0..=degree).rev() {
        if exponent != degree {
            shifted_degree = multiply_by_shifted_linear(&mut shifted, pole, shifted_degree)?;
        }
        let coefficient = Rat::from_poly(polynomial.coefficient_of(variable, exponent as i64)?);
        shifted[0] = add_rats(&shifted[0], &coefficient)?;
    }
    Ok(shifted)
}

/// Fast exact decomposition for proper fractions whose denominator splits
/// completely into linear factors over the current rational-function field.
///
/// At a pole `a` of multiplicity `m`, write
///
/// `f(a+t) = t^-m * N(a+t) / G(a+t)`,
///
/// where `G = D/(x-a)^m`.  Only the first `m` Taylor coefficients of `N/G`
/// are needed.  Building `G(a+t)` from the already factored denominator and
/// solving the triangular Cauchy product avoids the much larger global linear
/// system used by a general-purpose `apart` implementation.
fn try_linear_partial_fractions(
    function: &Rat,
    variable: usize,
) -> Result<Option<PartialFractionization>> {
    let ctx = function.ctx().clone();
    let numerator_degree = function.numerator().degree(variable)?;
    let denominator_degree = function.denominator().degree(variable)?;
    if numerator_degree >= denominator_degree {
        return Ok(None);
    }

    let factorization = linear_factors(function.denominator(), variable)?;
    if !factorization.nonlinear.is_empty() || factorization.linear.is_empty() {
        return Ok(None);
    }

    // A square-free factorization should already have one entry per distinct
    // pole. Merge defensively so a zero pole difference can never make the
    // local cofactor's constant term vanish.
    let mut factors: Vec<LinearFactor> = Vec::with_capacity(factorization.linear.len());
    for factor in factorization.linear {
        if let Some(existing) = factors
            .iter_mut()
            .find(|existing| existing.pole.equal(&factor.pole))
        {
            existing.multiplicity = existing
                .multiplicity
                .checked_add(factor.multiplicity)
                .ok_or_else(|| {
                    Error::InvalidInput("partial-fraction multiplicity overflow".into())
                })?;
        } else {
            factors.push(factor);
        }
    }

    let denominator_constant = Rat::from_poly(factorization.constant);
    let mut output = PartialFractionization {
        polynomial_part: Rat::zero(ctx.clone()),
        poles: Vec::with_capacity(factors.len()),
    };

    for (pole_index, factor) in factors.iter().enumerate() {
        let terms = factor.multiplicity;
        let numerator_series =
            shifted_polynomial_series(function.numerator(), variable, &factor.pole, terms)?;

        let mut cofactor_series = vec![Rat::zero(ctx.clone()); terms];
        cofactor_series[0] = denominator_constant.clone();
        let mut cofactor_degree = 0;
        for (other_index, other) in factors.iter().enumerate() {
            if pole_index == other_index {
                continue;
            }
            let pole_difference = sub_rats(&factor.pole, &other.pole)?;
            if pole_difference.is_zero() {
                // This can only arise if an exotic factorization presents the
                // same root in non-identical canonical forms. Let native apart
                // handle that representation instead of dividing by zero.
                return Ok(None);
            }
            for _ in 0..other.multiplicity {
                cofactor_degree = multiply_by_shifted_linear(
                    &mut cofactor_series,
                    &pole_difference,
                    cofactor_degree,
                )?;
            }
        }

        if cofactor_series[0].is_zero() {
            return Ok(None);
        }

        // N/G = h_0 + h_1*t + ... modulo t^m. The coefficient h_n
        // multiplies 1/(x-a)^(m-n), hence the reversal below.
        let mut quotient_series = Vec::with_capacity(terms);
        for exponent in 0..terms {
            let mut value = numerator_series[exponent].clone();
            for cofactor_exponent in 1..=exponent {
                let product = mul_rats(
                    &cofactor_series[cofactor_exponent],
                    &quotient_series[exponent - cofactor_exponent],
                )?;
                value = sub_rats(&value, &product)?;
            }
            quotient_series.push(value.try_div(&cofactor_series[0])?);
        }

        let mut coefs = vec![Rat::zero(ctx.clone()); terms];
        for (series_exponent, coefficient) in quotient_series.into_iter().enumerate() {
            coefs[terms - series_exponent - 1] = coefficient;
        }
        output.poles.push(PartialFractionPole {
            pole: factor.pole.clone(),
            multiplicity: factor.multiplicity,
            coefs,
        });
    }

    canonicalize_poles(&mut output);
    Ok(Some(output))
}

/// Divide in the selected variable over the rational-function coefficient
/// field, returning the polynomial quotient and the proper remainder.
///
/// Multivariate polynomial division is not sufficient here: the leading
/// coefficients may depend on the other variables and therefore need to be
/// inverted. This mirrors the quotient extraction used by Symbolica's native
/// rational-polynomial `apart` implementation.
fn polynomial_part_and_proper_remainder(function: &Rat, variable: usize) -> Result<(Rat, Rat)> {
    if function.numerator().degree(variable)? < function.denominator().degree(variable)? {
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
/// Completely linear denominators use the Taylor/Cauchy recurrence below.
/// Remaining inputs fall back to Symbolica's factored-denominator `apart`
/// output. Both routes retain multiplicities and keep `coefs` in HyperFLINT's
/// ascending order `c_1, ..., c_m`.
pub fn partial_fractions(function: &Rat, variable: usize) -> Result<PartialFractionization> {
    partial_fractions_with_options(function, variable, &PartialFractionOptions::default())
}

/// Compute a Symbolica-backed decomposition, optionally extending the
/// coefficient alphabet by formal roots of irreducible quadratics.
///
/// Fractions with a completely linear denominator use an exact local
/// Taylor/Cauchy recurrence over Symbolica rational functions. In algebraic
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
