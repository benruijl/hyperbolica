//! Projectivity detection and validated scan-input augmentation.

use std::collections::HashSet;

use crate::core::Poly;
use crate::error::{Error, Result};

use super::ScanExponent;

fn homogeneous_degree(polynomial: &Poly, variables: &[usize]) -> Option<i64> {
    if polynomial.is_zero() {
        return None;
    }
    let mut degree = None;
    for exponents in polynomial.inner().exponents_iter() {
        let term_degree = variables
            .iter()
            .map(|variable| i64::from(exponents[*variable]))
            .sum::<i64>();
        if degree.is_some_and(|first| first != term_degree) {
            return None;
        }
        degree = Some(term_degree);
    }
    degree
}

/// Detect projectivity group by group: all factors must be homogeneous in
/// the integration variables and both epsilon-weighted degree identities
/// must hold.
pub fn projective_input(
    group_polys: &[Vec<Poly>],
    xvar_indices: &[usize],
    exponents: &[Vec<ScanExponent>],
) -> Result<bool> {
    if group_polys.is_empty() || exponents.len() != group_polys.len() {
        return Ok(false);
    }
    let n = i64::try_from(xvar_indices.len())
        .map_err(|_| Error::InvalidInput("too many integration variables".into()))?;
    let reference_variables = group_polys
        .iter()
        .flatten()
        .next()
        .map(|polynomial| polynomial.ctx().vars());
    for (group, powers) in group_polys.iter().zip(exponents) {
        if group.len() != powers.len() {
            return Ok(false);
        }
        let mut sum_a = 0_i64;
        let mut sum_b = 0_i64;
        for (polynomial, exponent) in group.iter().zip(powers) {
            if reference_variables.is_some_and(|vars| polynomial.ctx().vars() != vars) {
                return Err(Error::ContextMismatch);
            }
            for &variable in xvar_indices {
                if variable >= polynomial.ctx().len() {
                    return Err(Error::UnknownVariable(variable.to_string()));
                }
            }
            let Some(degree) = homogeneous_degree(polynomial, xvar_indices) else {
                return Ok(false);
            };
            sum_a =
                sum_a
                    .checked_add(exponent.a.checked_mul(degree).ok_or_else(|| {
                        Error::InvalidInput("projectivity degree overflow".into())
                    })?)
                    .ok_or_else(|| Error::InvalidInput("projectivity degree overflow".into()))?;
            sum_b =
                sum_b
                    .checked_add(exponent.b.checked_mul(degree).ok_or_else(|| {
                        Error::InvalidInput("projectivity degree overflow".into())
                    })?)
                    .ok_or_else(|| Error::InvalidInput("projectivity degree overflow".into()))?;
        }
        if sum_a != -n || sum_b != 0 {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn augment_groups(
    group_polys: &[Vec<Poly>],
    xvar_indices: &[usize],
) -> Result<Vec<Vec<Poly>>> {
    let context = group_polys
        .iter()
        .flatten()
        .next()
        .ok_or_else(|| Error::InvalidInput("projective scan has no polynomials".into()))?
        .ctx()
        .clone();
    let mut seen_variables = HashSet::new();
    for &variable in xvar_indices {
        if variable >= context.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if !seen_variables.insert(variable) {
            return Err(Error::InvalidInput(
                "scan integration variable indices must be unique".into(),
            ));
        }
    }

    let mut augmented = group_polys.to_vec();
    for group in &mut augmented {
        for &variable in xvar_indices {
            group.push(Poly::generator(context.clone(), variable)?);
        }
    }
    Ok(augmented)
}
