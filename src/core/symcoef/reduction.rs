use super::SymCoef;
use crate::core::{Poly, Rat, global_period_table};
use crate::error::{Error, Result};
use crate::reduce::MzvReductionTable;

pub(super) fn simplify_symcoef(
    coefficient: &SymCoef,
    table: &MzvReductionTable,
) -> Result<SymCoef> {
    if coefficient
        .terms()
        .iter()
        .all(|term| matches!(term.pi_power, 0 | 1))
    {
        return Ok(coefficient.clone());
    }
    let mzv_two = crate::symbols::mzv_atom(&[2]);
    let mzv_two_index = coefficient.ctx().index_of_indeterminate(mzv_two.as_view());
    let tuple_mode =
        mzv_two_index.is_none() && crate::reduce::period_tuples_active(coefficient.ctx(), table);
    if mzv_two_index.is_none() && !tuple_mode {
        return Ok(coefficient.clone());
    }
    let six_mzv2 = mzv_two_index
        .map(|index| {
            Rat::from_poly(Poly::generator(coefficient.ctx().clone(), index)?)
                .try_mul(&Rat::from_int(coefficient.ctx().clone(), 6))
        })
        .transpose()?;
    let period_id = tuple_mode
        .then(|| global_period_table().id_for("mzv_2"))
        .transpose()?;
    let six = Rat::from_int(coefficient.ctx().clone(), 6);
    let mut monomials = Vec::with_capacity(coefficient.terms().len());
    for mut monomial in coefficient.terms().iter().cloned() {
        // Euclidean division also gives the mathematically correct reduction
        // for negative powers: Pi^-2 = (6*mzv_2)^-1.
        let pairs = monomial.pi_power.div_euclid(2);
        monomial.pi_power = monomial.pi_power.rem_euclid(2);
        if pairs != 0 {
            if let Some(six_mzv2) = &six_mzv2 {
                monomial.prefactor = monomial
                    .prefactor
                    .try_mul(&six_mzv2.pow(i64::from(pairs))?)?;
            } else if let Some(period_id) = period_id {
                monomial.prefactor = monomial.prefactor.try_mul(&six.pow(i64::from(pairs))?)?;
                let power = monomial.period_powers.entry(period_id).or_insert(0);
                *power = power.checked_add(pairs).ok_or_else(|| {
                    Error::InvalidInput("period power overflow during Pi reduction".into())
                })?;
            }
        }
        monomials.push(monomial);
    }
    SymCoef::try_from_monomials(coefficient.ctx().clone(), monomials)
}

pub(super) fn reduce_to_rat(coefficient: &SymCoef, table: &MzvReductionTable) -> Result<Rat> {
    let simplified = simplify_symcoef(coefficient, table)?;
    for monomial in simplified.terms() {
        let residual = if monomial.pi_power != 0 {
            Some(format!("residual Pi^{}", monomial.pi_power))
        } else if monomial.i_power != 0 {
            Some("residual imaginary unit I".into())
        } else if !monomial.log_powers.is_empty() {
            Some("residual Log factor".into())
        } else if !monomial.delta_powers.is_empty() {
            Some("residual delta factor".into())
        } else if !monomial.period_powers.is_empty() {
            Some("residual period generator".into())
        } else {
            None
        };
        if let Some(residual) = residual {
            return Err(Error::InvalidInput(format!("reduce_to_rat: {residual}")));
        }
    }
    simplified.as_rat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::symcoef_to_atom;
    use crate::core::{PolyCtx, SymMonomial};
    use crate::reduce::standard_mzv_reductions;
    use symbolica::prelude::{Atom, AtomCore};

    #[test]
    fn negative_pi_powers_reduce_exactly_in_tuple_contexts() {
        let ctx = PolyCtx::new(["negative_pi_tuple_x"]).unwrap();
        let table = standard_mzv_reductions();
        for exponent in [-3, -2, -1] {
            let mut term = SymMonomial::new(Rat::one(ctx.clone()));
            term.pi_power = exponent;
            let value = SymCoef::from_monomials(ctx.clone(), vec![term]);
            let reduced = simplify_symcoef(&value, &table).unwrap();
            let expected = (Atom::num(6) * crate::symbols::mzv_atom(&[2]))
                .pow(exponent.div_euclid(2))
                * symbolica::prelude::Symbol::PI
                    .to_atom()
                    .pow(exponent.rem_euclid(2));
            assert_eq!(symcoef_to_atom(&reduced).unwrap(), expected);
        }
    }

    #[test]
    fn pi_reduction_rejects_period_power_overflow() {
        let ctx = PolyCtx::new(["pi_tuple_overflow_x"]).unwrap();
        let mut term = SymMonomial::new(Rat::one(ctx.clone()));
        term.pi_power = 2;
        term.period_powers
            .insert(global_period_table().id_for("mzv_2").unwrap(), i32::MAX);
        let value = SymCoef::from_monomials(ctx, vec![term]);
        assert!(matches!(
            simplify_symcoef(&value, &standard_mzv_reductions()),
            Err(Error::InvalidInput(message)) if message.contains("overflow")
        ));
    }
}
