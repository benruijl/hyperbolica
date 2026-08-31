use std::collections::{BTreeMap, btree_map::Entry};
use std::sync::Arc;

use super::evaluation::zero_inf_period;
use super::{FibrationBasisResult, FibrationBasisResultSym};
use crate::core::{PolyCtx, Rat, SymCoef};
use crate::error::{Error, Result};
use crate::integrator::{
    RegKey, RegTerm, Regulator, RegulatorSym, canonicalize_regkey, regkey_content_key,
    transform_shuffle,
};
use crate::symbols::Wordlist;

use crate::reduce::mzv_reduce::MzvReductionTable;

fn fibration_vars(ctx: &Arc<PolyCtx>, var_indices: &[usize]) -> Result<Vec<String>> {
    var_indices
        .iter()
        .map(|&variable| {
            ctx.vars()
                .get(variable)
                .cloned()
                .ok_or_else(|| Error::UnknownVariable(variable.to_string()))
        })
        .collect()
}

fn advance_cartesian(counter: &mut [usize], sizes: &[usize]) -> bool {
    for (index, size) in sizes.iter().copied().enumerate() {
        if counter[index] + 1 == size {
            counter[index] = 0;
        } else {
            counter[index] += 1;
            return true;
        }
    }
    false
}

#[derive(Default)]
struct FibBasisAcc {
    entries: BTreeMap<String, (RegKey, Rat)>,
}

impl FibBasisAcc {
    fn add(&mut self, key: RegKey, coefficient: Rat) -> Result<()> {
        if coefficient.is_zero() {
            return Ok(());
        }
        let content_key = regkey_content_key(&key);
        match self.entries.entry(content_key) {
            Entry::Vacant(entry) => {
                entry.insert((key, coefficient));
            }
            Entry::Occupied(mut entry) => {
                let sum = entry.get().1.try_add(&coefficient)?;
                if sum.is_zero() {
                    entry.remove();
                } else {
                    entry.get_mut().1 = sum;
                }
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct FibBasisAccSym {
    entries: BTreeMap<String, (RegKey, SymCoef)>,
}

impl FibBasisAccSym {
    fn add(&mut self, key: RegKey, coefficient: SymCoef) -> Result<()> {
        if coefficient.is_zero() {
            return Ok(());
        }
        let content_key = regkey_content_key(&key);
        match self.entries.entry(content_key) {
            Entry::Vacant(entry) => {
                entry.insert((key, coefficient));
            }
            Entry::Occupied(mut entry) => {
                let sum = entry.get().1.try_add(&coefficient)?;
                if sum.is_zero() {
                    entry.remove();
                } else {
                    entry.get_mut().1 = sum;
                }
            }
        }
        Ok(())
    }
}

struct FibBasisWorker<'a> {
    ctx: &'a Arc<PolyCtx>,
    table: &'a MzvReductionTable,
    accumulator: FibBasisAcc,
}

impl FibBasisWorker<'_> {
    fn emit_prefix_product(&mut self, prefix: &[Wordlist], coefficient: &Rat) -> Result<()> {
        let sizes = prefix
            .iter()
            .map(|wordlist| wordlist.terms.len())
            .collect::<Vec<_>>();
        if sizes.contains(&0) {
            return Ok(());
        }

        let mut counter = vec![0; prefix.len()];
        loop {
            let mut key = Vec::with_capacity(prefix.len());
            let mut contribution = coefficient.clone();
            for (slot, alternative) in counter.iter().copied().enumerate() {
                let term = &prefix[slot].terms[alternative];
                key.push(term.word.clone());
                contribution = contribution.try_mul(&term.coef)?;
            }
            self.accumulator.add(key, contribution)?;
            if !advance_cartesian(&mut counter, &sizes) {
                break;
            }
        }
        Ok(())
    }

    fn base_case(
        &mut self,
        regulator: &Regulator,
        prefix: &[Wordlist],
        value_factor: &Rat,
    ) -> Result<()> {
        let mut value = Rat::zero(self.ctx.clone());
        for term in regulator {
            let mut product = Rat::one(self.ctx.clone());
            for word in &term.key {
                if word.is_empty() {
                    continue;
                }
                let period = zero_inf_period(self.ctx, word, self.table).map_err(|error| {
                    Error::InvalidInput(format!(
                        "fibration_basis: base-case period is not evaluable: {error}"
                    ))
                })?;
                product = product.try_mul(&period)?;
            }
            value = value.try_add(&term.coef.try_mul(&product)?)?;
        }
        self.emit_prefix_product(prefix, &value.try_mul(value_factor)?)
    }

    fn recurse(
        &mut self,
        regulator: &Regulator,
        variables: &[usize],
        prefix: &mut Vec<Wordlist>,
        value_factor: &Rat,
    ) -> Result<()> {
        let Some((&variable, rest)) = variables.split_first() else {
            return self.base_case(regulator, prefix, value_factor);
        };

        for term in regulator {
            for transformed in transform_shuffle(self.ctx, &term.key, variable)? {
                let mut inner = Regulator::with_capacity(transformed.regulator.len());
                for inner_term in transformed.regulator {
                    let coefficient = inner_term.coef.as_rat().map_err(|_| {
                        Error::InvalidInput(
                            "fibration_basis: transform_shuffle emitted a non-rational symbolic coefficient"
                                .into(),
                        )
                    })?;
                    inner.push(RegTerm {
                        coef: coefficient,
                        key: inner_term.key,
                    });
                }

                let next_factor = value_factor.try_mul(&term.coef)?;
                prefix.push(transformed.shuffle);
                let result = self.recurse(&inner, rest, prefix, &next_factor);
                prefix.pop();
                result?;
            }
        }
        Ok(())
    }
}

struct FibBasisWorkerSym<'a> {
    ctx: &'a Arc<PolyCtx>,
    table: &'a MzvReductionTable,
    accumulator: FibBasisAccSym,
}

impl FibBasisWorkerSym<'_> {
    fn emit_prefix_product(
        &mut self,
        prefix: &[Wordlist],
        extra_key: &RegKey,
        coefficient: &SymCoef,
    ) -> Result<()> {
        let sizes = prefix
            .iter()
            .map(|wordlist| wordlist.terms.len())
            .collect::<Vec<_>>();
        if sizes.contains(&0) {
            return Ok(());
        }

        let mut counter = vec![0; prefix.len()];
        loop {
            // This ordering intentionally follows the upstream implementation:
            // terminal passthrough words precede transformed-variable slots.
            let mut key = Vec::with_capacity(extra_key.len() + prefix.len());
            key.extend(extra_key.iter().cloned());
            let mut contribution = coefficient.clone();
            for (slot, alternative) in counter.iter().copied().enumerate() {
                let term = &prefix[slot].terms[alternative];
                key.push(term.word.clone());
                contribution = contribution.try_mul_rat(&term.coef)?;
            }
            self.accumulator.add(key, contribution)?;
            if !advance_cartesian(&mut counter, &sizes) {
                break;
            }
        }
        Ok(())
    }

    fn base_case(
        &mut self,
        regulator: &RegulatorSym,
        prefix: &[Wordlist],
        value_factor: &SymCoef,
    ) -> Result<()> {
        for term in regulator {
            let entry_coefficient = value_factor.try_mul(&term.coef)?;
            let mut period_product = Rat::one(self.ctx.clone());
            let mut evaluable = true;
            for word in &term.key {
                if word.is_empty() {
                    continue;
                }
                match zero_inf_period(self.ctx, word, self.table) {
                    Ok(period) => period_product = period_product.try_mul(&period)?,
                    Err(_) => {
                        evaluable = false;
                        break;
                    }
                }
            }

            if evaluable {
                let coefficient = entry_coefficient.try_mul_rat(&period_product)?;
                self.emit_prefix_product(prefix, &RegKey::new(), &coefficient)?;
            } else {
                let passthrough = canonicalize_regkey(&term.key);
                self.emit_prefix_product(prefix, &passthrough, &entry_coefficient)?;
            }
        }
        Ok(())
    }

    fn recurse(
        &mut self,
        regulator: &RegulatorSym,
        variables: &[usize],
        prefix: &mut Vec<Wordlist>,
        value_factor: &SymCoef,
    ) -> Result<()> {
        let Some((&variable, rest)) = variables.split_first() else {
            return self.base_case(regulator, prefix, value_factor);
        };

        for term in regulator {
            for transformed in transform_shuffle(self.ctx, &term.key, variable)? {
                let next_factor = value_factor.try_mul(&term.coef)?;
                prefix.push(transformed.shuffle);
                let result = self.recurse(&transformed.regulator, rest, prefix, &next_factor);
                prefix.pop();
                result?;
            }
        }
        Ok(())
    }
}

pub(super) fn fibration_basis(
    ctx: &Arc<PolyCtx>,
    input: &Regulator,
    var_indices: &[usize],
    table: &MzvReductionTable,
) -> Result<FibrationBasisResult> {
    let vars = fibration_vars(ctx, var_indices)?;
    let mut worker = FibBasisWorker {
        ctx,
        table,
        accumulator: FibBasisAcc::default(),
    };
    worker.recurse(
        input,
        var_indices,
        &mut Vec::with_capacity(var_indices.len()),
        &Rat::one(ctx.clone()),
    )?;
    Ok(FibrationBasisResult {
        vars,
        terms: worker.accumulator.entries.into_values().collect(),
    })
}

pub(super) fn fibration_basis_sym(
    ctx: &Arc<PolyCtx>,
    input: &RegulatorSym,
    var_indices: &[usize],
    table: &MzvReductionTable,
) -> Result<FibrationBasisResultSym> {
    let vars = fibration_vars(ctx, var_indices)?;
    let mut worker = FibBasisWorkerSym {
        ctx,
        table,
        accumulator: FibBasisAccSym::default(),
    };
    worker.recurse(
        input,
        var_indices,
        &mut Vec::with_capacity(var_indices.len()),
        &SymCoef::one(ctx.clone()),
    )?;
    Ok(FibrationBasisResultSym {
        vars,
        terms: worker.accumulator.entries.into_values().collect(),
    })
}

pub(super) fn test_zero_function_sym(
    ctx: &Arc<PolyCtx>,
    regulator: &RegulatorSym,
    var_indices: &[usize],
    table: &MzvReductionTable,
) -> Result<bool> {
    Ok(fibration_basis_sym(ctx, regulator, var_indices, table)?
        .terms
        .iter()
        .all(|(_, coefficient)| coefficient.is_zero()))
}
