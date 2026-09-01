//! Integer lifting, finite-field specialization, and constraint handling.

use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use symbolica::prelude::*;

use crate::core::Poly;

use super::{FieldPoly, IntegerPoly, MAX_SECTOR_VARIABLES, RABINOWITSCH_VARIABLE};

pub(super) fn primitive_integer(polynomial: &Poly) -> IntegerPoly {
    if polynomial.is_zero() {
        return MultivariatePolynomial::new(&Z, None, polynomial.inner().get_vars());
    }
    let denominator_lcm = polynomial
        .inner()
        .coefficients
        .iter()
        .fold(Integer::one(), |lcm, coefficient| {
            lcm.lcm(coefficient.denominator_ref())
        });
    polynomial
        .inner()
        .map_coeff(
            |coefficient| {
                Z.mul(
                    coefficient.numerator_ref(),
                    &Z.quot(&denominator_lcm, coefficient.denominator_ref()),
                )
            },
            Z,
        )
        .make_primitive()
}

pub(super) fn validate_indices(
    factors: &[Poly],
    exponents: &[i64],
    propagator_variables: &[usize],
    parameter_variables: &[usize],
    constraint: Option<&Poly>,
) -> bool {
    if factors.is_empty()
        || factors.len() != exponents.len()
        || propagator_variables.len() > MAX_SECTOR_VARIABLES
    {
        return false;
    }
    let Some(first) = factors.first() else {
        return false;
    };
    if factors
        .iter()
        .any(|factor| !factor.ctx().is_compatible_with(first.ctx()))
        || constraint.is_some_and(|value| !value.ctx().is_compatible_with(first.ctx()))
    {
        return false;
    }
    let mut seen = HashSet::with_capacity(propagator_variables.len() + parameter_variables.len());
    if propagator_variables
        .iter()
        .chain(parameter_variables)
        .any(|variable| *variable >= first.ctx().len() || !seen.insert(*variable))
    {
        return false;
    }
    factors.iter().chain(constraint).all(|polynomial| {
        polynomial
            .used_variable_indices()
            .iter()
            .all(|variable| seen.contains(variable))
    })
}

pub(super) fn finite_field_image(polynomial: &IntegerPoly, field: &Zp) -> FieldPoly {
    polynomial.map_coeff(|coefficient| field.nth(coefficient.clone()), field.clone())
}

pub(super) fn choose_constraint_variable(
    constraint: &IntegerPoly,
    propagator_variables: &[usize],
    parameter_variables: &[usize],
) -> Option<usize> {
    if propagator_variables
        .iter()
        .any(|variable| constraint.degree(*variable) != 0)
    {
        return None;
    }
    parameter_variables
        .iter()
        .copied()
        .filter(|variable| constraint.degree(*variable) != 0)
        .min_by_key(|variable| constraint.degree(*variable))
}

pub(super) fn constraint_root(
    constraint: &IntegerPoly,
    constraint_variable: usize,
    parameter_variables: &[usize],
    residues: &[u32],
    field: &Zp,
) -> Option<u32> {
    let mut univariate = finite_field_image(constraint, field);
    for variable in parameter_variables {
        if *variable != constraint_variable {
            univariate = univariate.replace(*variable, &field.to_element(residues[*variable]));
        }
    }
    if univariate.degree(constraint_variable) == 0 {
        return None;
    }

    let factors = catch_unwind(AssertUnwindSafe(|| univariate.factor())).ok()?;
    factors
        .into_iter()
        .filter_map(|(factor, _)| {
            if factor.degree(constraint_variable) != 1
                || (0..factor.nvars())
                    .any(|variable| variable != constraint_variable && factor.degree(variable) != 0)
            {
                return None;
            }
            let root = field.neg(&field.div(&factor.get_constant(), &factor.lcoeff()));
            Some(field.from_element(&root))
        })
        .min()
}

pub(super) fn specialize_and_project(
    polynomial: &IntegerPoly,
    active_propagators: &[usize],
    inactive_propagators: &[usize],
    parameter_variables: &[usize],
    residues: &[u32],
    field: &Zp,
) -> FieldPoly {
    let mut specialized = finite_field_image(polynomial, field);
    let zero = field.zero();
    for variable in inactive_propagators {
        specialized = specialized.replace(*variable, &zero);
    }
    for variable in parameter_variables {
        specialized = specialized.replace(*variable, &field.to_element(residues[*variable]));
    }

    let mut variables = Vec::with_capacity(active_propagators.len() + 1);
    variables.push(RABINOWITSCH_VARIABLE);
    variables.extend(
        active_propagators
            .iter()
            .map(|variable| polynomial.get_vars_ref()[*variable].clone()),
    );
    let mut projected = FieldPoly::new(field, Some(specialized.nterms()), Arc::new(variables));
    let mut exponents = vec![0_u16; active_propagators.len() + 1];
    for monomial in &specialized {
        for (target, source) in active_propagators.iter().copied().enumerate() {
            exponents[target + 1] = monomial.exponents[source];
        }
        projected.append_monomial(*monomial.coefficient, &exponents);
    }
    projected
}
