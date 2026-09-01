//! Blockwise partial fractions from a deferred denominator product.

use crate::core::{FactoredRat, Rat};
use crate::error::{Error, Result};

use super::{
    PartialFractionOptions, PartialFractionization, canonicalize_poles, merge_decomposition,
    partial_fractions_with_options,
};
use crate::algebra::algebraic_letters::join_algebraic_letter_session;

/// Decompose a [`FactoredRat`] blockwise when its target-dependent denominator
/// blocks are pairwise coprime.
///
/// Each separated block is materialized as a canonical [`Rat`] before the
/// ordinary partial-fraction adapter runs. Overlapping input blocks use the
/// exact eager full-materialization fallback.
pub fn partial_fractions_factored(
    function: &FactoredRat,
    variable: usize,
) -> Result<PartialFractionization> {
    partial_fractions_factored_with_options(function, variable, &PartialFractionOptions::default())
}

/// Decompose a deferred denominator with Symbolica-backed exact arithmetic and
/// public factorization/partial-fraction primitives.
///
/// With algebraic-letter introduction enabled, Hyperbolica materializes the
/// complete function once so all quadratic blocks are split in one formal
/// coefficient field. Otherwise, `FactorizedRationalPolynomial::apart`
/// separates already-known pairwise-coprime target-dependent denominator
/// blocks. Hyperbolica eagerly materializes each resulting component (one
/// target-dependent block plus all target-independent factors) and hands it to
/// the ordinary adapter. Linear components use its Taylor/Cauchy recurrence;
/// nonlinear components fall back to
/// `RationalPolynomial::apart_factored_denominators`. The pinned native `apart`
/// still expands powered blocks and intermediate suffix products internally;
/// callers must treat this as a benchmarked blockwise route, not as a promise
/// that every intermediate remains factored. If callers supplied overlapping
/// target-dependent blocks, the function instead materializes the complete
/// rational function once so correctness is unchanged.
pub fn partial_fractions_factored_with_options(
    function: &FactoredRat,
    variable: usize,
    options: &PartialFractionOptions<'_>,
) -> Result<PartialFractionization> {
    if variable >= function.ctx().len() {
        return Err(Error::UnknownVariable(variable.to_string()));
    }

    let _session = options
        .introduce_algebraic_letters
        .then(join_algebraic_letter_session)
        .transpose()?;
    if options.introduce_algebraic_letters {
        // Algebraic roots from separate blockwise decompositions live in one
        // formal coefficient field, so the blocks cannot safely be split in
        // isolation. Materialize once and let the ordinary adapter replace
        // every admissible quadratic in one operation while this session is
        // held through the complete result construction.
        return partial_fractions_with_options(&function.materialize()?, variable, options);
    }

    let Some(components) = function.apart_components(variable)? else {
        return partial_fractions_with_options(&function.materialize()?, variable, options);
    };
    let mut output = PartialFractionization {
        polynomial_part: Rat::zero(function.ctx().clone()),
        poles: Vec::new(),
    };
    for component in components {
        merge_decomposition(
            &mut output,
            partial_fractions_with_options(&component, variable, options)?,
        )?;
    }
    canonicalize_poles(&mut output);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algebra::algebraic_letters::{
        AlgebraicLetterTable, DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE, begin_algebraic_letter_session,
        build_algebraic_letter_atom_list,
    };
    use crate::core::{Poly, PolyCtx};
    use crate::symbols::SYMBOL_NAMESPACE;
    use symbolica::prelude::Symbol;

    fn reconstruct(result: &PartialFractionization, variable: usize) -> Rat {
        let ctx = result.polynomial_part.ctx().clone();
        let x = Rat::from_poly(Poly::generator(ctx, variable).unwrap());
        let mut value = result.polynomial_part.clone();
        for pole in &result.poles {
            let base = x.try_sub(&pole.pole).unwrap();
            for (order, coefficient) in pole.coefs.iter().enumerate() {
                value = value
                    .try_add(
                        &coefficient
                            .try_div(&base.pow((order + 1) as i64).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
            }
        }
        value
    }

    fn assert_cross_product_equal(left: &Rat, right: &Rat) {
        assert_eq!(
            left.native().numerator.clone() * &right.native().denominator,
            right.native().numerator.clone() * &left.native().denominator
        );
    }

    fn algebraic_context() -> std::sync::Arc<PolyCtx> {
        let variables = ["pf_fact_alg_x", "pf_fact_alg_a", "pf_fact_alg_b"]
            .into_iter()
            .map(|name| Symbol::parse(name, SYMBOL_NAMESPACE).unwrap().to_atom());
        PolyCtx::from_indeterminates(build_algebraic_letter_atom_list(
            variables,
            DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE,
        ))
        .unwrap()
    }

    fn build(
        ctx: &std::sync::Arc<PolyCtx>,
        numerator: &str,
        factors: &[(&str, i64)],
    ) -> FactoredRat {
        let mut function = FactoredRat::from_poly(Poly::parse(ctx.clone(), numerator).unwrap());
        for &(base, exponent) in factors {
            function
                .push_factor(&Poly::parse(ctx.clone(), base).unwrap(), exponent)
                .unwrap();
        }
        function
    }

    fn assert_equivalent(function: &FactoredRat, variable: usize) {
        let factored = partial_fractions_factored(function, variable).unwrap();
        let materialized = function.materialize().unwrap();
        let ordinary = super::super::partial_fractions(&materialized, variable).unwrap();
        assert_eq!(reconstruct(&factored, variable), materialized);
        assert_eq!(
            reconstruct(&factored, variable),
            reconstruct(&ordinary, variable)
        );
    }

    #[test]
    fn upstream_shaped_many_repeated_blocks_match_materialized_oracle() {
        let ctx = PolyCtx::new(["x", "a", "b", "c"]).unwrap();
        let function = build(
            &ctx,
            "x^9+a*x^7+b*x^4+c*x^2+a*b*c+1",
            &[("2*x-a", 4), ("x+b", 3), ("3*x+c", 2), ("x+a+b+c+1", 1)],
        );
        assert_equivalent(&function, 0);

        let decomposition = partial_fractions_factored(&function, 0).unwrap();
        let mut multiplicities = decomposition
            .poles
            .iter()
            .map(|pole| pole.multiplicity)
            .collect::<Vec<_>>();
        multiplicities.sort_unstable();
        assert_eq!(multiplicities, [1, 2, 3, 4]);
    }

    #[test]
    fn rational_units_and_target_independent_factors_are_exact() {
        let ctx = PolyCtx::new(["x", "y", "z"]).unwrap();
        let function = build(
            &ctx,
            "(x+3*y+1)/5",
            &[
                ("(x+y)/2", 3),
                ("(3*x-z)/7", 2),
                ("(y+z+1)/11", 4),
                ("-13/17", 3),
            ],
        );
        assert_equivalent(&function, 0);
    }

    #[test]
    fn proportional_blocks_merge_and_general_overlaps_fall_back_exactly() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let proportional = build(&ctx, "x^3+y+1", &[("2*x+2", 2), ("-3*x-3", 3), ("x+y", 1)]);
        assert_equivalent(&proportional, 0);

        let overlapping = build(&ctx, "x^4+y*x+2", &[("x+1", 2), ("x^2-1", 3), ("x+y+2", 1)]);
        assert!(overlapping.apart_components(0).unwrap().is_none());
        assert_equivalent(&overlapping, 0);
    }

    #[test]
    fn zero_polynomial_and_invalid_variable_edges_match_rat_api() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let zero = FactoredRat::from_poly(Poly::zero(ctx.clone()));
        let result = partial_fractions_factored(&zero, 0).unwrap();
        assert!(result.polynomial_part.is_zero());
        assert!(result.poles.is_empty());

        let polynomial = FactoredRat::from_poly(Poly::parse(ctx.clone(), "x^4+y*x+1").unwrap());
        let result = partial_fractions_factored(&polynomial, 0).unwrap();
        assert_eq!(result.polynomial_part, polynomial.materialize().unwrap());
        assert!(result.poles.is_empty());

        let error = partial_fractions_factored(&zero, 2).unwrap_err();
        assert!(matches!(error, Error::UnknownVariable(variable) if variable == "2"));
    }

    #[test]
    fn linear_hot_path_keeps_every_rat_compatibility_view_cold() {
        let ctx = PolyCtx::new(["x", "y", "z"]).unwrap();
        let function = build(&ctx, "x^5+y*x+z", &[("x+y", 3), ("2*x-z", 2), ("x+1", 1)]);
        let result = partial_fractions_factored(&function, 0).unwrap();

        assert!(!result.polynomial_part.compatibility_views_initialized());
        for pole in result.poles {
            assert!(!pole.pole.compatibility_views_initialized());
            assert!(
                pole.coefs
                    .iter()
                    .all(|coefficient| !coefficient.compatibility_views_initialized())
            );
        }
    }

    #[test]
    fn deterministic_block_matrix_is_equivalent() {
        let ctx = PolyCtx::new(["x", "y", "z"]).unwrap();
        for seed in 1_i64..=16 {
            let numerator = format!(
                "x^{}+{}*y*x^2+{}*z+1",
                3 + seed % 5,
                1 + seed % 7,
                2 + seed % 11
            );
            let first = format!("{}*x+y+{}", 1 + seed % 3, 1 + seed);
            let second = format!("{}*x-z-{}", 2 + seed % 5, 2 + seed);
            let third = format!("x+y+z+{}", 3 + seed);
            let function = build(
                &ctx,
                &numerator,
                &[
                    (&first, 1 + seed % 4),
                    (&second, 1 + (seed + 1) % 3),
                    (&third, 1 + (seed + 2) % 2),
                ],
            );
            assert_equivalent(&function, 0);
        }
    }

    #[test]
    fn pole_order_is_independent_of_deferred_factor_insertion_order() {
        let ctx = PolyCtx::new(["x", "a", "b"]).unwrap();
        let numerator = "x^4+a*x^2+b*x+1";
        let forward = build(&ctx, numerator, &[("2*x-a", 3), ("x+b", 2), ("3*x+1", 1)]);
        let reverse = build(&ctx, numerator, &[("3*x+1", 1), ("x+b", 2), ("2*x-a", 3)]);

        let forward = partial_fractions_factored(&forward, 0).unwrap();
        let reverse = partial_fractions_factored(&reverse, 0).unwrap();

        assert_eq!(forward, reverse);
        assert_eq!(
            forward
                .poles
                .iter()
                .map(|pole| pole.multiplicity)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
    }

    #[test]
    fn algebraic_two_quadratic_blocks_are_split_as_one_materialized_function() {
        let _session = begin_algebraic_letter_session().unwrap();
        let ctx = algebraic_context();
        let function = build(
            &ctx,
            "1",
            &[
                ("pf_fact_alg_x^2-pf_fact_alg_a", 1),
                ("pf_fact_alg_x^2-pf_fact_alg_b", 1),
            ],
        );
        let options = PartialFractionOptions {
            introduce_algebraic_letters: true,
            forbidden_variables: &[],
        };
        let result = partial_fractions_factored_with_options(&function, 0, &options).unwrap();

        assert_eq!(result.poles.len(), 4);
        assert!(result.poles.iter().all(|pole| pole.multiplicity == 1));
        assert_eq!(AlgebraicLetterTable::global().size().unwrap(), 2);

        let x = Rat::from_poly(Poly::generator(ctx.clone(), 0).unwrap());
        let mut split_denominator = Rat::one(ctx.clone());
        for pole in &result.poles {
            split_denominator = split_denominator
                .try_mul(&x.try_sub(&pole.pole).unwrap())
                .unwrap();
        }
        let expected = Rat::one(ctx).try_div(&split_denominator).unwrap();
        assert_cross_product_equal(&reconstruct(&result, 0), &expected);
    }
}
