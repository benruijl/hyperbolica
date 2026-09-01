//! Algebraic-root ratio combination and square-root back-substitution.

use symbolica::prelude::Atom;

use crate::core::{Poly, PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::reduce::substitute_var_rat;

use super::AlgebraicLetterEntry;
use super::registry::algebraic_letters_show;

pub(super) fn find_var_idx(ctx: &PolyCtx, atom: &Atom) -> Option<usize> {
    ctx.index_of_indeterminate(atom.as_view())
}

fn algebraic_atom_total(value: &Rat, entries: &[AlgebraicLetterEntry]) -> Result<i64> {
    let mut total = 0_i64;
    for entry in entries {
        let atoms = entry.atoms();
        for atom in [&atoms.minus, &atoms.plus, &atoms.ratio] {
            let Some(variable) = find_var_idx(value.ctx(), atom) else {
                continue;
            };
            let numerator_degree = value.numerator_degree(variable)?.max(0);
            let denominator_degree = value.denominator_degree(variable)?.max(0);
            total = total
                .checked_add(numerator_degree)
                .and_then(|sum| sum.checked_add(denominator_degree))
                .ok_or_else(|| Error::InvalidInput("algebraic atom count overflowed".into()))?;
        }
    }
    Ok(total)
}

/// Replace a shrinking `Wm_i/Wp_i` ratio by `WmOverWp_i`.
pub fn combine_wm_wp_ratios(value: &Rat) -> Result<Rat> {
    let entries = algebraic_letters_show()?;
    let mut current = value.clone();
    for entry in &entries {
        let atoms = entry.atoms();
        let (Some(wm), Some(wp), Some(ratio)) = (
            find_var_idx(value.ctx(), &atoms.minus),
            find_var_idx(value.ctx(), &atoms.plus),
            find_var_idx(value.ctx(), &atoms.ratio),
        ) else {
            continue;
        };

        if current.numerator_degree(wm)? <= 0 || current.denominator_degree(wp)? <= 0 {
            continue;
        }
        let replacement = Rat::from_poly(Poly::generator(value.ctx().clone(), ratio)?)
            .try_mul(&Rat::from_poly(Poly::generator(value.ctx().clone(), wp)?))?;
        let candidate = substitute_var_rat(&current, wm, &replacement)?;
        if algebraic_atom_total(&candidate, &entries)? < algebraic_atom_total(&current, &entries)? {
            current = candidate;
        }
    }
    Ok(current)
}

/// Materialize every `Wm_i`/`Wp_i` through a symbolic square-root atom.
pub fn back_substitute(value: &Rat) -> Result<Rat> {
    let entries = algebraic_letters_show()?;
    let half =
        Rat::from_int(value.ctx().clone(), 1).try_div(&Rat::from_int(value.ctx().clone(), 2))?;
    let mut current = value.clone();

    for entry in &entries {
        let atoms = entry.atoms();
        let wm = find_var_idx(value.ctx(), &atoms.minus);
        let wp = find_var_idx(value.ctx(), &atoms.plus);
        let sqrt_disc = find_var_idx(value.ctx(), &atoms.sqrt_discriminant);
        let (Some(wm), Some(wp), Some(sqrt_disc)) = (wm, wp, sqrt_disc) else {
            return Err(Error::InvalidInput(format!(
                "back_substitute: context is missing registered Wm/Wp/sqrt_disc atoms for pair {} — call build_algebraic_letter_atom_list when constructing the context",
                entry.idx
            )));
        };

        let sqrt_disc = Rat::from_poly(Poly::generator(value.ctx().clone(), sqrt_disc)?);
        let sum_half = entry.sum_value.try_mul(&half)?;
        let sqrt_over_two_lc = sqrt_disc.try_mul(&half)?.try_div(&entry.lc)?;
        let wm_replacement = sum_half.try_sub(&sqrt_over_two_lc)?;
        let wp_replacement = sum_half.try_add(&sqrt_over_two_lc)?;
        current = substitute_var_rat(&current, wm, &wm_replacement)?;
        current = substitute_var_rat(&current, wp, &wp_replacement)?;
    }
    Ok(current)
}
