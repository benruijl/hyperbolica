use symbolica::domains::rational_polynomial::FromNumeratorAndDenominator;
use symbolica::prelude::Z;

use crate::core::{NativeRat, Poly, Rat};
use crate::error::{Error, Result};

use super::algebraic_introduction::introduce_quadratic;
use super::algebraic_letters::join_algebraic_letter_session;

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

/// Compute a univariate partial-fraction decomposition using Symbolica's
/// factored-denominator `apart` implementation.
///
/// Denominator multiplicities are taken directly from Symbolica's
/// `(numerator, denominator_base, exponent)` output.  This avoids expanding
/// powers merely to recover them again and keeps `coefs` in HyperFLINT's
/// ascending order `c_1, ..., c_m`.
pub fn partial_fractions(function: &Rat, variable: usize) -> Result<PartialFractionization> {
    partial_fractions_with_options(function, variable, &PartialFractionOptions::default())
}

/// Compute a Symbolica-native decomposition, optionally extending the
/// coefficient alphabet by formal roots of irreducible quadratics.
///
/// Symbolica's native `apart_factored_denominators` remains the only partial-
/// fraction engine. For a quadratic component, this adapter replaces its
/// denominator base by the exact formal factorization
/// `lc*(x-Wm(i))*(x-Wp(i))` and invokes the same native engine again.
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
mod tests {
    use super::*;

    use symbolica::prelude::{AtomCore, Symbol};

    use crate::algebra::algebraic_letters::{
        AlgebraicLetterTable, DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE, begin_algebraic_letter_session,
        build_algebraic_letter_atom_list,
    };
    use crate::core::{Poly, PolyCtx};
    use crate::symbols::SYMBOL_NAMESPACE;

    fn reconstruct(result: &PartialFractionization, variable: usize) -> Rat {
        let ctx = result.polynomial_part.ctx().clone();
        let x = Rat::from_poly(Poly::generator(ctx, variable).unwrap());
        let mut value = result.polynomial_part.clone();
        for pole in &result.poles {
            assert_eq!(pole.coefs.len(), pole.multiplicity);
            let base = x.try_sub(&pole.pole).unwrap();
            for (order, coefficient) in pole.coefs.iter().enumerate() {
                let term = coefficient
                    .try_div(&base.pow((order + 1) as i64).unwrap())
                    .unwrap();
                value = value.try_add(&term).unwrap();
            }
        }
        value
    }

    fn algebraic_context() -> std::sync::Arc<PolyCtx> {
        let variables = ["pf_alg_x", "pf_alg_a", "pf_alg_b", "pf_alg_z"]
            .into_iter()
            .map(|name| Symbol::parse(name, SYMBOL_NAMESPACE).unwrap().to_atom());
        PolyCtx::from_indeterminates(build_algebraic_letter_atom_list(
            variables,
            DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE,
        ))
        .unwrap()
    }

    fn algebraic_options<'a>(forbidden_variables: &'a [usize]) -> PartialFractionOptions<'a> {
        PartialFractionOptions {
            introduce_algebraic_letters: true,
            forbidden_variables,
        }
    }

    #[test]
    fn simple_poles_and_polynomial_part_reconstruct() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let function = Rat::parse(ctx, "(x^3+2*x+1)/((x-1)*(x+2))").unwrap();
        let result = partial_fractions(&function, 0).unwrap();

        assert_eq!(result.poles.len(), 2);
        assert!(result.poles.iter().all(|pole| pole.multiplicity == 1));
        assert_eq!(reconstruct(&result, 0), function);
    }

    #[test]
    fn repeated_pole_coefficients_are_in_ascending_order() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let function = Rat::parse(ctx.clone(), "(x^3+2*x+y)/(x-y)^2").unwrap();
        let result = partial_fractions(&function, 0).unwrap();

        assert_eq!(result.poles.len(), 1);
        assert_eq!(result.poles[0].pole, Rat::parse(ctx.clone(), "y").unwrap());
        assert_eq!(result.poles[0].multiplicity, 2);
        assert_eq!(
            result.poles[0].coefs,
            vec![
                Rat::parse(ctx.clone(), "3*y^2+2").unwrap(),
                Rat::parse(ctx.clone(), "y^3+3*y").unwrap(),
            ]
        );
        assert_eq!(result.polynomial_part, Rat::parse(ctx, "x+2*y").unwrap());
        assert_eq!(reconstruct(&result, 0), function);
    }

    #[test]
    fn rational_parameter_pole_reconstructs_with_leading_coefficient_scaling() {
        let ctx = PolyCtx::new(["x", "y", "z"]).unwrap();
        let function = Rat::parse(ctx.clone(), "(x^2+y*z+z^2)/(z*x-y)^3").unwrap();
        let result = partial_fractions(&function, 0).unwrap();

        assert!(result.polynomial_part.is_zero());
        assert_eq!(result.poles.len(), 1);
        let pole = &result.poles[0];
        assert_eq!(pole.pole, Rat::parse(ctx.clone(), "y/z").unwrap());
        assert_eq!(pole.multiplicity, 3);
        assert_eq!(
            pole.coefs,
            vec![
                Rat::parse(ctx.clone(), "1/z^3").unwrap(),
                Rat::parse(ctx.clone(), "2*y/z^4").unwrap(),
                Rat::parse(ctx, "(y^2+y*z^3+z^4)/z^5").unwrap(),
            ]
        );
        assert_eq!(reconstruct(&result, 0), function);
    }

    #[test]
    fn nonlinear_denominator_is_reported() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let function = Rat::parse(ctx, "1/(x^2+y)").unwrap();
        let error = partial_fractions(&function, 0).unwrap_err();
        assert!(error.to_string().contains("degree 2"));
    }

    #[test]
    fn zero_and_polynomial_inputs_have_only_a_polynomial_part() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let zero = partial_fractions(&Rat::zero(ctx.clone()), 0).unwrap();
        assert!(zero.polynomial_part.is_zero());
        assert!(zero.poles.is_empty());

        let polynomial = Rat::parse(ctx.clone(), "x^4+y*x+1").unwrap();
        let result = partial_fractions(&polynomial, 0).unwrap();
        assert_eq!(result.polynomial_part, polynomial);
        assert!(result.poles.is_empty());

        let cancelled = Rat::parse(ctx, "(x^2-1)/(x-1)").unwrap();
        let result = partial_fractions(&cancelled, 0).unwrap();
        assert_eq!(result.polynomial_part, cancelled);
        assert!(result.poles.is_empty());
    }

    #[test]
    fn several_scaled_repeated_poles_reconstruct_exactly() {
        let ctx = PolyCtx::new(["x", "y", "z"]).unwrap();
        let function =
            Rat::parse(ctx, "(x^7+y*x^5+z*x^3+y*z*x+1)/((2*x-y)^3*(x+z)^2*(3*x+1))").unwrap();
        let result = partial_fractions(&function, 0).unwrap();

        assert_eq!(result.poles.len(), 3);
        let mut multiplicities = result
            .poles
            .iter()
            .map(|pole| pole.multiplicity)
            .collect::<Vec<_>>();
        multiplicities.sort_unstable();
        assert_eq!(multiplicities, [1, 2, 3]);
        assert!(result.poles.iter().all(|pole| {
            pole.coefs.iter().all(|coefficient| {
                coefficient.numerator().degree(0).unwrap() <= 0
                    && coefficient.denominator().degree(0).unwrap() <= 0
            })
        }));
        assert_eq!(reconstruct(&result, 0), function);
    }

    #[test]
    fn invalid_variable_is_reported_before_trivial_returns() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let error = partial_fractions(&Rat::zero(ctx), 1).unwrap_err();
        assert!(matches!(error, Error::UnknownVariable(variable) if variable == "1"));
    }

    #[test]
    fn irreducible_quadratic_is_split_into_registered_function_poles() {
        let _session = begin_algebraic_letter_session().unwrap();
        let ctx = algebraic_context();
        let function = Rat::parse(ctx.clone(), "1/(pf_alg_x^2-pf_alg_a)").unwrap();
        let result = partial_fractions_with_options(&function, 0, &algebraic_options(&[])).unwrap();

        assert!(result.polynomial_part.is_zero());
        assert_eq!(result.poles.len(), 2);
        assert!(result.poles.iter().all(|pole| pole.multiplicity == 1));
        assert!(result.poles.iter().all(|pole| {
            let atom = pole.pole.to_atom();
            atom.as_fun_view().is_some()
        }));

        let x = Rat::from_poly(Poly::generator(ctx.clone(), 0).unwrap());
        let split = x
            .try_sub(&result.poles[0].pole)
            .unwrap()
            .try_mul(&x.try_sub(&result.poles[1].pole).unwrap())
            .unwrap();
        let expected = Rat::one(ctx).try_div(&split).unwrap();
        assert_eq!(reconstruct(&result, 0), expected);

        let entry = AlgebraicLetterTable::global().at(1).unwrap();
        assert_eq!(entry.polynomial, function.denominator().clone());
        assert_eq!(entry.sum_value, Rat::zero(entry.polynomial.ctx().clone()));
        assert_eq!(
            entry.product_value,
            Rat::parse(entry.polynomial.ctx().clone(), "-pf_alg_a").unwrap()
        );
    }

    #[test]
    fn repeated_quadratic_and_linear_factors_reconstruct_in_the_split_alphabet() {
        let _session = begin_algebraic_letter_session().unwrap();
        let ctx = algebraic_context();
        let numerator = Rat::parse(ctx.clone(), "pf_alg_x+pf_alg_b").unwrap();
        let function = Rat::parse(
            ctx.clone(),
            "(pf_alg_x+pf_alg_b)/((pf_alg_x^2-pf_alg_a)^2*(pf_alg_x-pf_alg_b))",
        )
        .unwrap();
        let result = partial_fractions_with_options(&function, 0, &algebraic_options(&[])).unwrap();

        assert_eq!(result.poles.len(), 3);
        let mut multiplicities = result
            .poles
            .iter()
            .map(|pole| pole.multiplicity)
            .collect::<Vec<_>>();
        multiplicities.sort_unstable();
        assert_eq!(multiplicities, [1, 2, 2]);

        let x = Rat::from_poly(Poly::generator(ctx.clone(), 0).unwrap());
        let algebraic = result
            .poles
            .iter()
            .filter(|pole| pole.multiplicity == 2)
            .collect::<Vec<_>>();
        let b = Rat::from_poly(Poly::generator(ctx.clone(), 2).unwrap());
        let split_denominator = x
            .try_sub(&algebraic[0].pole)
            .unwrap()
            .pow(2)
            .unwrap()
            .try_mul(&x.try_sub(&algebraic[1].pole).unwrap().pow(2).unwrap())
            .unwrap()
            .try_mul(&x.try_sub(&b).unwrap())
            .unwrap();
        let expected = numerator.try_div(&split_denominator).unwrap();
        assert_eq!(reconstruct(&result, 0), expected);
    }

    #[test]
    fn parameter_leading_coefficient_is_preserved_exactly() {
        let _session = begin_algebraic_letter_session().unwrap();
        let ctx = algebraic_context();
        let function =
            Rat::parse(ctx.clone(), "1/(pf_alg_z*pf_alg_x^2+pf_alg_x+pf_alg_a)").unwrap();
        let result = partial_fractions_with_options(&function, 0, &algebraic_options(&[])).unwrap();
        assert_eq!(result.poles.len(), 2);

        let x = Rat::from_poly(Poly::generator(ctx.clone(), 0).unwrap());
        let z = Rat::from_poly(Poly::generator(ctx.clone(), 3).unwrap());
        let split_denominator = z
            .try_mul(&x.try_sub(&result.poles[0].pole).unwrap())
            .unwrap()
            .try_mul(&x.try_sub(&result.poles[1].pole).unwrap())
            .unwrap();
        assert_eq!(
            reconstruct(&result, 0),
            Rat::one(ctx).try_div(&split_denominator).unwrap()
        );
    }

    #[test]
    fn remaining_variable_dependency_is_a_clear_failure_without_allocation() {
        let _session = begin_algebraic_letter_session().unwrap();
        let ctx = algebraic_context();
        let function = Rat::parse(ctx, "1/(pf_alg_x^2-pf_alg_a)").unwrap();
        let error =
            partial_fractions_with_options(&function, 0, &algebraic_options(&[1])).unwrap_err();
        assert!(error.to_string().contains("remaining integration variable"));
        assert_eq!(AlgebraicLetterTable::global().size().unwrap(), 0);
    }
}
