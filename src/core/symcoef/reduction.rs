use super::SymCoef;
use crate::core::{Poly, Rat};
use crate::error::{Error, Result};
use crate::reduce::MzvReductionTable;

pub(super) fn simplify_symcoef(
    coefficient: &SymCoef,
    _table: &MzvReductionTable,
) -> Result<SymCoef> {
    let mzv_two = crate::symbols::mzv_atom(&[2]);
    let Some(mzv_two_index) = coefficient.ctx().index_of_indeterminate(mzv_two.as_view()) else {
        return Ok(coefficient.clone());
    };
    let six_mzv2 = Rat::from_poly(Poly::generator(coefficient.ctx().clone(), mzv_two_index)?)
        .try_mul(&Rat::from_int(coefficient.ctx().clone(), 6))?;
    let mut monomials = Vec::with_capacity(coefficient.terms().len());
    for mut monomial in coefficient.terms().iter().cloned() {
        // Euclidean division also gives the mathematically correct reduction
        // for negative powers: Pi^-2 = (6*mzv_2)^-1.
        let pairs = monomial.pi_power.div_euclid(2);
        monomial.pi_power = monomial.pi_power.rem_euclid(2);
        if pairs != 0 {
            monomial.prefactor = monomial
                .prefactor
                .try_mul(&six_mzv2.pow(i64::from(pairs))?)?;
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
