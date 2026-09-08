use std::sync::Arc;

use super::evaluation::zero_inf_period;
use super::{FibrationBasisResult, FibrationBasisResultSym};
use crate::core::{DigestBuckets, PolyCtx, Rat, SymCoef, structural_bucket_digest};
use crate::error::{Error, Result};
use crate::integrator::{
    RegKey, RegTerm, Regulator, RegulatorSym, TransformOptions, TransformSession,
    canonicalize_regkey, regkey_content_key, regkey_structural_cmp,
};
use crate::symbols::Wordlist;

use crate::reduce::mzv_reduce::MzvReductionTable;
use crate::reduce::period_scratch::{mint_period_sym, period_tuples_active};

const FIBRATION_REGKEY_BUCKET_DOMAIN: u64 = 0x4649_4252_4547_0001;

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

fn sort_entries_for_presentation<V>(entries: Vec<(RegKey, V)>) -> Vec<(RegKey, V)> {
    let mut decorated = entries
        .into_iter()
        .map(|entry| (regkey_content_key(&entry.0), entry))
        .collect::<Vec<_>>();
    decorated.sort_unstable_by(|(left_key, left), (right_key, right)| {
        left_key
            .cmp(right_key)
            .then_with(|| regkey_structural_cmp(&left.0, &right.0))
    });
    decorated.into_iter().map(|(_, entry)| entry).collect()
}

#[derive(Default)]
pub(super) struct FibBasisAcc {
    buckets: DigestBuckets,
    entries: Vec<(RegKey, Rat)>,
}

impl FibBasisAcc {
    pub(super) fn add(&mut self, key: RegKey, coefficient: Rat) -> Result<()> {
        let key = canonicalize_regkey(&key);
        let digest = structural_bucket_digest(FIBRATION_REGKEY_BUCKET_DOMAIN, &key);
        self.add_canonical_with_digest(key, coefficient, digest)
    }

    #[cfg(test)]
    pub(super) fn add_with_digest(
        &mut self,
        key: RegKey,
        coefficient: Rat,
        digest: u64,
    ) -> Result<()> {
        let key = canonicalize_regkey(&key);
        self.add_canonical_with_digest(key, coefficient, digest)
    }

    fn add_canonical_with_digest(
        &mut self,
        key: RegKey,
        coefficient: Rat,
        digest: u64,
    ) -> Result<()> {
        if coefficient.is_zero() {
            return Ok(());
        }
        if let Some(index) = self.buckets.find(digest, |index| {
            self.entries
                .get(index)
                .is_some_and(|(candidate, _)| candidate == &key)
        }) {
            let entry = &mut self.entries[index];
            entry.1 = entry.1.try_add(&coefficient)?;
        } else {
            let index = self.entries.len();
            self.entries.push((key, coefficient));
            self.buckets.insert(digest, index);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn storage_shape(&self, digest: u64) -> (usize, usize) {
        (self.entries.len(), self.buckets.bucket_len(digest))
    }

    pub(super) fn into_terms(self) -> Vec<(RegKey, Rat)> {
        // Retain the established lexical wire/presentation ordering after all
        // semantic collection has completed structurally. Full structural
        // order resolves equal-spelling presentation collisions.
        sort_entries_for_presentation(
            self.entries
                .into_iter()
                .filter(|(_, coefficient)| !coefficient.is_zero())
                .collect(),
        )
    }
}

#[derive(Default)]
pub(super) struct FibBasisAccSym {
    buckets: DigestBuckets,
    entries: Vec<(RegKey, SymCoef)>,
}

impl FibBasisAccSym {
    pub(super) fn add(&mut self, key: RegKey, coefficient: SymCoef) -> Result<()> {
        let key = canonicalize_regkey(&key);
        let digest = structural_bucket_digest(FIBRATION_REGKEY_BUCKET_DOMAIN, &key);
        self.add_canonical_with_digest(key, coefficient, digest)
    }

    #[cfg(test)]
    pub(super) fn add_with_digest(
        &mut self,
        key: RegKey,
        coefficient: SymCoef,
        digest: u64,
    ) -> Result<()> {
        let key = canonicalize_regkey(&key);
        self.add_canonical_with_digest(key, coefficient, digest)
    }

    fn add_canonical_with_digest(
        &mut self,
        key: RegKey,
        coefficient: SymCoef,
        digest: u64,
    ) -> Result<()> {
        if coefficient.is_zero() {
            return Ok(());
        }
        if let Some(index) = self.buckets.find(digest, |index| {
            self.entries
                .get(index)
                .is_some_and(|(candidate, _)| candidate == &key)
        }) {
            let entry = &mut self.entries[index];
            entry.1 = entry.1.try_add(&coefficient)?;
        } else {
            let index = self.entries.len();
            self.entries.push((key, coefficient));
            self.buckets.insert(digest, index);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn storage_shape(&self, digest: u64) -> (usize, usize) {
        (self.entries.len(), self.buckets.bucket_len(digest))
    }

    pub(super) fn into_terms(self) -> Vec<(RegKey, SymCoef)> {
        // Retain the established lexical wire/presentation ordering after all
        // semantic collection has completed structurally. Full structural
        // order resolves equal-spelling presentation collisions.
        sort_entries_for_presentation(
            self.entries
                .into_iter()
                .filter(|(_, coefficient)| !coefficient.is_zero())
                .collect(),
        )
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

        let mut transforms = TransformSession::new(
            self.ctx,
            variable,
            TransformOptions::default(),
            Some(self.table),
        )?;
        for term in regulator {
            for transformed in transforms.transform(&term.key)? {
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
        let tuple_mode = period_tuples_active(self.ctx, self.table);
        for term in regulator {
            let entry_coefficient = value_factor.try_mul(&term.coef)?;
            let mut period_product = SymCoef::one(self.ctx.clone());
            let mut evaluable = true;
            for word in &term.key {
                if word.is_empty() {
                    continue;
                }
                let period = if tuple_mode {
                    mint_period_sym(self.ctx, word, self.table, false)
                } else {
                    zero_inf_period(self.ctx, word, self.table)
                        .map(|value| SymCoef::from_rat(&value))
                };
                match period {
                    Ok(period) => period_product = period_product.try_mul(&period)?,
                    Err(_) => {
                        evaluable = false;
                        break;
                    }
                }
            }

            if evaluable {
                let coefficient = entry_coefficient.try_mul(&period_product)?;
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

        let mut transforms = TransformSession::new(
            self.ctx,
            variable,
            TransformOptions::default(),
            Some(self.table),
        )?;
        for term in regulator {
            for transformed in transforms.transform(&term.key)? {
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
        terms: worker.accumulator.into_terms(),
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
        terms: worker.accumulator.into_terms(),
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
