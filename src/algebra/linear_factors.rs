use crate::core::{Poly, Rat};
use crate::error::Result;

use super::algebraic_introduction::introduce_quadratic;
use super::algebraic_letters::join_algebraic_letter_session;

/// Policy for factoring one integration variable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinearFactorOptions<'a> {
    /// Replace irreducible quadratic factors by registered `Wm(i)`/`Wp(i)`
    /// roots.
    pub introduce_algebraic_letters: bool,
    /// Variables that remain to be integrated. Quadratics depending on any
    /// of them stay nonlinear, matching HyperIntica's conservative guard.
    pub forbidden_variables: &'a [usize],
}

/// A factor that is linear in the selected variable.
///
/// `pole` is the root `a` after normalizing the factor to `x - a`.
/// Its leading coefficient is absorbed into [`LinearFactorization::constant`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinearFactor {
    pub multiplicity: usize,
    pub pole: Rat,
}

/// An irreducible factor of degree at least two in the selected variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NonlinearFactor {
    pub multiplicity: usize,
    pub polynomial: Poly,
    pub degree_in_var: i64,
}

/// A factorization classified by degree in one selected variable.
///
/// If the selected variable is `x`, this represents
///
/// `constant * product((x - pole)^multiplicity) * product(nonlinear^multiplicity)`.
///
/// The constant remains an exact Symbolica-backed polynomial because, just as
/// in HyperFLINT, it can depend on variables other than `x`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinearFactorization {
    pub constant: Poly,
    pub linear: Vec<LinearFactor>,
    pub nonlinear: Vec<NonlinearFactor>,
}

/// Factor `polynomial` with Symbolica and classify its factors by degree in
/// `variable`.
///
/// For a linear base `c1*x + c0`, the returned pole is `-c0/c1`.  The
/// factor's `c1` is raised to its multiplicity and folded into `constant`, so
/// rational and parameter-dependent poles retain the same reconstruction
/// convention as HyperFLINT.
pub fn linear_factors(polynomial: &Poly, variable: usize) -> Result<LinearFactorization> {
    linear_factors_with_options(polynomial, variable, &LinearFactorOptions::default())
}

/// Factor and classify with optional formal quadratic-root introduction.
pub fn linear_factors_with_options(
    polynomial: &Poly,
    variable: usize,
    options: &LinearFactorOptions<'_>,
) -> Result<LinearFactorization> {
    let _session = options
        .introduce_algebraic_letters
        .then(join_algebraic_letter_session)
        .transpose()?;
    // Validate the variable even for zero and constant inputs.
    polynomial.degree(variable)?;

    let factorization = polynomial.factor();
    let mut constant = Poly::from_rational(polynomial.ctx().clone(), factorization.constant);
    let mut linear = Vec::new();
    let mut nonlinear = Vec::new();

    for (factor, multiplicity) in factorization.factors {
        let degree_in_var = factor.degree(variable)?;
        match degree_in_var {
            0 => {
                constant = constant.try_mul(&factor.pow(multiplicity))?;
            }
            1 => {
                let constant_coefficient = factor.coefficient_of(variable, 0)?;
                let leading_coefficient = factor.coefficient_of(variable, 1)?;
                let pole = Rat::new(-&constant_coefficient, leading_coefficient.clone())?;

                // c1*x + c0 = c1*(x - pole).  Keeping c1 here makes the
                // public factorization reconstruct exactly even when c1 is a
                // polynomial in the remaining variables.
                constant = constant.try_mul(&leading_coefficient.pow(multiplicity))?;
                linear.push(LinearFactor { multiplicity, pole });
            }
            2 if options.introduce_algebraic_letters => {
                let Some(introduced) =
                    introduce_quadratic(&factor, variable, options.forbidden_variables)?
                else {
                    nonlinear.push(NonlinearFactor {
                        multiplicity,
                        polynomial: factor,
                        degree_in_var,
                    });
                    continue;
                };

                // f = lc * (x-Wm) * (x-Wp). Keep the exact public
                // reconstruction convention even when lc depends on other
                // parameters.
                let leading_coefficient = factor.coefficient_of(variable, 2)?;
                constant = constant.try_mul(&leading_coefficient.pow(multiplicity))?;
                linear.push(LinearFactor {
                    multiplicity,
                    pole: introduced.minus,
                });
                linear.push(LinearFactor {
                    multiplicity,
                    pole: introduced.plus,
                });
            }
            _ => nonlinear.push(NonlinearFactor {
                multiplicity,
                polynomial: factor,
                degree_in_var,
            }),
        }
    }

    Ok(LinearFactorization {
        constant,
        linear,
        nonlinear,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use symbolica::prelude::{AtomCore, Symbol};

    use crate::algebra::algebraic_letters::{
        AlgebraicLetterTable, DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE, begin_algebraic_letter_session,
        build_algebraic_letter_atom_list,
    };
    use crate::core::{PolyCtx, Rat};
    use crate::symbols::SYMBOL_NAMESPACE;

    use super::*;

    fn reconstruct(factors: &LinearFactorization, variable: usize, ctx: Arc<PolyCtx>) -> Rat {
        let x = Rat::from_poly(Poly::generator(ctx.clone(), variable).unwrap());
        let mut value = Rat::from_poly(factors.constant.clone());

        for factor in &factors.linear {
            let base = x.try_sub(&factor.pole).unwrap();
            value = value
                .try_mul(&base.pow(factor.multiplicity as i64).unwrap())
                .unwrap();
        }
        for factor in &factors.nonlinear {
            value = value
                .try_mul(&Rat::from_poly(factor.polynomial.pow(factor.multiplicity)))
                .unwrap();
        }
        value
    }

    fn algebraic_context() -> Arc<PolyCtx> {
        let variables = ["lf_alg_x", "lf_alg_a", "lf_alg_z"]
            .into_iter()
            .map(|name| Symbol::parse(name, SYMBOL_NAMESPACE).unwrap().to_atom());
        PolyCtx::from_indeterminates(build_algebraic_letter_atom_list(
            variables,
            DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE,
        ))
        .unwrap()
    }

    #[test]
    fn classifies_multiplicities_and_parameter_dependent_poles() {
        let ctx = PolyCtx::new(["x", "y", "z"]).unwrap();
        let polynomial = Poly::parse(ctx.clone(), "(z*x-y)^3*(2*x+3*y)^2*(x^2+y)*(y+1)").unwrap();

        let factors = linear_factors(&polynomial, 0).unwrap();
        assert_eq!(factors.linear.len(), 2);
        assert_eq!(factors.nonlinear.len(), 1);

        let rational_pole = factors
            .linear
            .iter()
            .find(|factor| factor.multiplicity == 3)
            .unwrap();
        assert_eq!(rational_pole.pole, Rat::parse(ctx.clone(), "y/z").unwrap());

        let repeated_pole = factors
            .linear
            .iter()
            .find(|factor| factor.multiplicity == 2)
            .unwrap();
        assert_eq!(
            repeated_pole.pole,
            Rat::parse(ctx.clone(), "-3*y/2").unwrap()
        );

        assert_eq!(factors.nonlinear[0].degree_in_var, 2);
        assert_eq!(factors.nonlinear[0].multiplicity, 1);
        assert_eq!(reconstruct(&factors, 0, ctx), Rat::from_poly(polynomial));
    }

    #[test]
    fn target_independent_factors_are_folded_into_constant() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let polynomial = Poly::parse(ctx.clone(), "3*y^2*(x-y)").unwrap();
        let factors = linear_factors(&polynomial, 0).unwrap();

        assert_eq!(factors.linear.len(), 1);
        assert!(factors.nonlinear.is_empty());
        assert_eq!(reconstruct(&factors, 0, ctx), Rat::from_poly(polynomial));
    }

    #[test]
    fn quadratic_option_emits_two_structural_roots_and_absorbs_the_leading_coefficient() {
        let _session = begin_algebraic_letter_session().unwrap();
        let ctx = algebraic_context();
        let polynomial = Poly::parse(ctx.clone(), "lf_alg_z*lf_alg_x^2+lf_alg_x+lf_alg_a").unwrap();
        let factors = linear_factors_with_options(
            &polynomial,
            0,
            &LinearFactorOptions {
                introduce_algebraic_letters: true,
                forbidden_variables: &[],
            },
        )
        .unwrap();

        assert!(factors.nonlinear.is_empty());
        assert_eq!(factors.linear.len(), 2);
        assert_eq!(
            factors.constant,
            Poly::parse(ctx.clone(), "lf_alg_z").unwrap()
        );
        assert!(factors.linear.iter().all(|factor| {
            factor.pole.to_atom().as_fun_view().is_some_and(|call| {
                let heads = crate::symbols::heads();
                call.get_symbol() == heads.algebraic_minus
                    || call.get_symbol() == heads.algebraic_plus
            })
        }));
        assert_eq!(AlgebraicLetterTable::global().size().unwrap(), 1);
    }

    #[test]
    fn forbidden_quadratic_stays_nonlinear_and_does_not_allocate() {
        let _session = begin_algebraic_letter_session().unwrap();
        let ctx = algebraic_context();
        let polynomial = Poly::parse(ctx, "lf_alg_x^2-lf_alg_a").unwrap();
        let factors = linear_factors_with_options(
            &polynomial,
            0,
            &LinearFactorOptions {
                introduce_algebraic_letters: true,
                forbidden_variables: &[1],
            },
        )
        .unwrap();
        assert!(factors.linear.is_empty());
        assert_eq!(factors.nonlinear.len(), 1);
        assert_eq!(AlgebraicLetterTable::global().size().unwrap(), 0);
    }
}
