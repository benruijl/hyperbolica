//! Per-pair Vieta recurrence and conservative normal-form reduction.

use crate::core::{Poly, Rat};
use crate::error::Result;

use super::registry::algebraic_letters_show;
use super::substitution::find_var_idx;

fn reduce_var_via_recurrence(
    value: &Rat,
    variable: usize,
    sum_value: &Rat,
    product_value: &Rat,
) -> Result<Rat> {
    let max_degree = value.numerator().degree(variable)?;
    if max_degree < 2 {
        return Ok(value.clone());
    }

    let ctx = value.ctx().clone();
    let zero = Rat::zero(ctx.clone());
    let one = Rat::one(ctx.clone());
    let mut upper = Vec::with_capacity(max_degree as usize + 1);
    let mut lower = Vec::with_capacity(max_degree as usize + 1);
    upper.push(zero.clone());
    lower.push(one.clone());
    upper.push(one);
    lower.push(zero.clone());

    for degree in 2..=max_degree as usize {
        let next_upper = sum_value
            .try_mul(&upper[degree - 1])?
            .try_sub(&product_value.try_mul(&upper[degree - 2])?)?;
        let next_lower = sum_value
            .try_mul(&lower[degree - 1])?
            .try_sub(&product_value.try_mul(&lower[degree - 2])?)?;
        upper.push(next_upper);
        lower.push(next_lower);
    }

    let mut coefficient_of_var = zero.clone();
    let mut free_part = zero;
    for degree in 0..=max_degree {
        let coefficient = value.numerator().coefficient_of(variable, degree)?;
        if coefficient.is_zero() {
            continue;
        }
        let coefficient = Rat::new(coefficient, value.denominator().clone())?;
        coefficient_of_var =
            coefficient_of_var.try_add(&coefficient.try_mul(&upper[degree as usize])?)?;
        free_part = free_part.try_add(&coefficient.try_mul(&lower[degree as usize])?)?;
    }

    let generator = Rat::from_poly(Poly::generator(ctx, variable)?);
    free_part.try_add(&coefficient_of_var.try_mul(&generator)?)
}

/// Reduce algebraic atoms with the same per-pair Vieta normal form as Mma.
pub fn simplify_with_vieta(value: &Rat) -> Result<Rat> {
    let entries = algebraic_letters_show()?;
    if entries.is_empty() {
        return Ok(value.clone());
    }

    // HyperIntica gives up globally when any allocated Wm/Wp occurs in the
    // denominator.  Partial fractions may be used by callers to clear it.
    for entry in &entries {
        let atoms = entry.atoms();
        for atom in [&atoms.minus, &atoms.plus] {
            if let Some(variable) = find_var_idx(value.ctx(), atom)
                && value.denominator().degree(variable)? > 0
            {
                return Ok(value.clone());
            }
        }
    }

    let half =
        Rat::from_int(value.ctx().clone(), 1).try_div(&Rat::from_int(value.ctx().clone(), 2))?;
    let mut expression = value.clone();
    for entry in &entries {
        let atoms = entry.atoms();
        let (Some(wm), Some(wp)) = (
            find_var_idx(value.ctx(), &atoms.minus),
            find_var_idx(value.ctx(), &atoms.plus),
        ) else {
            continue;
        };

        let mut expanded =
            reduce_var_via_recurrence(&expression, wm, &entry.sum_value, &entry.product_value)?;
        expanded =
            reduce_var_via_recurrence(&expanded, wp, &entry.sum_value, &entry.product_value)?;

        let numerator = expanded.numerator();
        let denominator = expanded.denominator();
        let num_no_wm = numerator.coefficient_of(wm, 0)?;
        let num_wm = numerator.coefficient_of(wm, 1)?;
        let a = Rat::new(num_no_wm.coefficient_of(wp, 0)?, denominator.clone())?;
        let c = Rat::new(num_no_wm.coefficient_of(wp, 1)?, denominator.clone())?;
        let b = Rat::new(num_wm.coefficient_of(wp, 0)?, denominator.clone())?;
        let d = Rat::new(num_wm.coefficient_of(wp, 1)?, denominator.clone())?;
        let collapsed_constant = a.try_add(&d.try_mul(&entry.product_value)?)?;

        // This is the upstream PossibleZeroQ gate.  If b+c is non-zero, the
        // per-pair candidate is discarded even though a more aggressive
        // quotient-ring simplifier could reduce it.
        if !b.try_add(&c)?.is_zero() {
            continue;
        }

        let wm_rat = Rat::from_poly(Poly::generator(value.ctx().clone(), wm)?);
        let wp_rat = Rat::from_poly(Poly::generator(value.ctx().clone(), wp)?);
        let antisymmetric = b
            .try_sub(&c)?
            .try_mul(&half)?
            .try_mul(&wm_rat.try_sub(&wp_rat)?)?;
        expression = collapsed_constant.try_add(&antisymmetric)?;
    }
    Ok(expression)
}
