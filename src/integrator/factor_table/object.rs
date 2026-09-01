//! Structural polynomial interning and exact factored-object construction.

use std::collections::BTreeMap;
use std::time::Instant;

use symbolica::prelude::Rational;

use crate::core::Poly;
use crate::error::{Error, Result};

use super::{FactorTable, FactoredObject};

pub(super) fn intern(table: &mut FactorTable, polynomial: &Poly) -> usize {
    let canonical = polynomial.canonical_proportional_form();
    table
        .intern_index
        .insert(&mut table.intern_polys, canonical)
        .0
}

fn fold_constant(accumulator: &mut Poly, value: &Poly, sign: i64) -> Result<()> {
    if !value.is_rational_constant() {
        return Err(Error::InvalidInput(
            "factor-table constant fold received a non-constant".into(),
        ));
    }
    if sign > 0 {
        *accumulator = accumulator.try_mul(value)?;
    } else {
        *accumulator = accumulator.div_exact(value)?;
    }
    Ok(())
}

fn factor_into(
    table: &mut FactorTable,
    target: &Poly,
    pool: &[usize],
    exponents: &mut BTreeMap<usize, i64>,
    constant: &mut Poly,
    sign: i64,
) -> Result<bool> {
    let mut work = target.clone();
    let trial_started = Instant::now();
    for &id in pool {
        if work.is_rational_constant() {
            break;
        }
        let divisor = table.intern_polys[id].clone();
        // Exact division by any non-zero constant in Q[x] always succeeds and
        // would never reduce the work polynomial's structure, so a malformed
        // trial pool containing a constant must not spin forever.
        if divisor.is_rational_constant() {
            continue;
        }
        loop {
            match work.div_exact(&divisor) {
                Ok(quotient) => {
                    work = quotient;
                    *exponents.entry(id).or_default() += sign;
                    if work.is_rational_constant() {
                        break;
                    }
                }
                Err(Error::InexactDivision) => break,
                Err(error) => return Err(error),
            }
        }
    }
    table.stats.trial_seconds += trial_started.elapsed().as_secs_f64();
    if work.is_rational_constant() {
        fold_constant(constant, &work, sign)?;
        return Ok(false);
    }

    let fallback_started = Instant::now();
    let factorization = work.factor();
    for (base, multiplicity) in factorization.factors {
        let canonical = base.canonical_proportional_form();
        let id = intern(table, &canonical);
        for _ in 0..multiplicity {
            work = work.div_exact(&canonical)?;
        }
        *exponents.entry(id).or_default() += sign * multiplicity as i64;
    }
    table.stats.fallback_seconds += fallback_started.elapsed().as_secs_f64();
    if !work.is_rational_constant() {
        return Err(Error::InvalidInput(
            "factor-table fallback left a non-constant remainder".into(),
        ));
    }
    fold_constant(constant, &work, sign)?;
    Ok(true)
}

pub(super) fn make_object(
    table: &mut FactorTable,
    numerators: &[Poly],
    denominators: &[Poly],
    pool: &[usize],
) -> Result<FactoredObject> {
    let Some(template) = numerators.first().or_else(|| denominators.first()) else {
        return Ok(FactoredObject::default());
    };
    if numerators.iter().any(Poly::is_zero) {
        return Ok(FactoredObject {
            constant: Rational::zero(),
            factors: Vec::new(),
            oop: false,
        });
    }
    let mut constant = Poly::one(template.ctx().clone());
    let mut exponents = BTreeMap::<usize, i64>::new();
    let mut oop = false;
    for numerator in numerators {
        if numerator.is_rational_constant() {
            fold_constant(&mut constant, numerator, 1)?;
        } else {
            oop |= factor_into(table, numerator, pool, &mut exponents, &mut constant, 1)?;
        }
    }
    for denominator in denominators {
        if denominator.is_zero() {
            return Err(Error::DivisionByZero);
        }
        if denominator.is_rational_constant() {
            fold_constant(&mut constant, denominator, -1)?;
        } else {
            oop |= factor_into(table, denominator, pool, &mut exponents, &mut constant, -1)?;
        }
    }
    if !constant.is_rational_constant() {
        return Err(Error::InvalidInput(
            "factor-table unit is not rational (internal error)".into(),
        ));
    }
    Ok(FactoredObject {
        constant: constant
            .rational_constant()
            .expect("factor-table unit was checked to be rational"),
        factors: exponents
            .into_iter()
            .filter(|(_, exponent)| *exponent != 0)
            .collect(),
        oop,
    })
}

#[cfg(test)]
mod tests {
    use crate::core::PolyCtx;

    use super::*;

    #[test]
    fn trial_pool_extracts_repeated_factors_from_one_exact_division_per_power() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let factor = Poly::parse(ctx.clone(), "x+y").unwrap();
        let target = Poly::from_int(ctx, 3).try_mul(&factor.pow(7)).unwrap();
        let mut table = FactorTable::default();
        let constant_id = intern(&mut table, &Poly::from_int(factor.ctx().clone(), 2));
        let id = intern(&mut table, &factor);

        let object = make_object(&mut table, &[target], &[], &[constant_id, id]).unwrap();

        assert_eq!(object.constant, Rational::from(3));
        assert_eq!(object.factors, vec![(id, 7)]);
        assert!(!object.oop);
        assert_eq!(table.stats.oop, 0);
    }
}
