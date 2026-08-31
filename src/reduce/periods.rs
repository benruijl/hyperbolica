//! Period evaluation for the MZV alphabets used by HyperFLINT.

use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};
use std::sync::Arc;

use crate::algebra::convert::{convert_one_infinity_to_zero_one, convert_zero_one};
use crate::core::{Poly, PolyCtx, Rat, SymCoef};
use crate::error::{Error, Result};
use crate::integrator::{
    RegKey, RegTerm, Regulator, RegulatorSym, canonicalize_regkey, canonicalize_regulator,
    reg_head, reg0, regkey_content_key, transform_shuffle,
};
use crate::symbols::{Word, Wordlist, WordlistTerm, log_two_atom, mzv_atom};

use super::mzv_expansion::{MzvExpansionTable, cross_ctx_transfer_rat};
use super::mzv_reduce::{MzvReductionTable, apply_mzv_reductions};

fn integer_letter(letter: &Rat, site: &str) -> Result<i64> {
    letter
        .to_string()
        .parse::<i64>()
        .map_err(|_| Error::InvalidInput(format!("{site}: non-integer letter `{letter}`")))
}

fn all_zero(word: &Word) -> bool {
    !word.is_empty() && word.letters.iter().all(Rat::is_zero)
}

fn mzv_name(indices: &[i64]) -> String {
    let encoded = indices
        .iter()
        .map(|index| {
            if *index < 0 {
                format!("m{}", index.unsigned_abs())
            } else {
                index.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("_");
    format!("mzv_{encoded}")
}

fn to_mzv_one_word(
    ctx: &Arc<PolyCtx>,
    coefficient: &Rat,
    word: &Word,
    expansion: Option<&MzvExpansionTable>,
) -> Result<Rat> {
    if word.is_empty() {
        return Ok(coefficient.clone());
    }
    if word[0].to_string() == "1" || word[word.len() - 1].is_zero() {
        // The period entry points regularize these cases before conversion.
        return Ok(Rat::zero(ctx.clone()));
    }

    let mut counts = Vec::<i64>::new();
    let mut poles = Vec::<i64>::new();
    for letter in word.letters.iter().rev() {
        if letter.is_zero() {
            if let Some(count) = counts.last_mut() {
                *count += 1;
            }
        } else {
            counts.push(1);
            poles.push(integer_letter(letter, "to_mzv")?);
        }
    }
    if counts.is_empty() {
        return Ok(Rat::zero(ctx.clone()));
    }
    poles.push(1);

    let mut indices = Vec::with_capacity(counts.len());
    for index in 0..counts.len() {
        let numerator = counts[index]
            .checked_mul(poles[index + 1])
            .ok_or_else(|| Error::InvalidInput("to_mzv: index overflow".into()))?;
        let denominator = poles[index];
        if denominator == 0 || numerator % denominator != 0 {
            return Err(Error::InvalidInput(
                "to_mzv: non-integral MZV index (unsupported alphabet)".into(),
            ));
        }
        indices.push(numerator / denominator);
    }

    let name = mzv_name(&indices);
    let native_atom = mzv_atom(&indices);
    let symbol = if let Some(variable) = ctx.index_of_indeterminate(native_atom.as_view()) {
        Rat::from_poly(Poly::generator(ctx.clone(), variable)?)
    } else if let Some(value) = expansion.and_then(|table| table.expansion.get(&name)) {
        cross_ctx_transfer_rat(value, ctx.clone())?
    } else {
        return Err(Error::UnknownVariable(format!(
            "{name} (required by to_mzv)"
        )));
    };
    let value = coefficient.try_mul(&symbol)?;
    Ok(if counts.len() & 1 == 0 {
        value
    } else {
        value.negated()
    })
}

/// Convert a literal-integer wordlist to its MZV expression.
pub fn to_mzv(ctx: &Arc<PolyCtx>, wordlist: &Wordlist) -> Result<Rat> {
    to_mzv_with_expansion(ctx, wordlist, None)
}

pub fn to_mzv_with_expansion(
    ctx: &Arc<PolyCtx>,
    wordlist: &Wordlist,
    expansion: Option<&MzvExpansionTable>,
) -> Result<Rat> {
    let mut result = Rat::zero(ctx.clone());
    for term in &wordlist.terms {
        result = result.try_add(&to_mzv_one_word(ctx, &term.coef, &term.word, expansion)?)?;
    }
    Ok(result)
}

/// Evaluate a period over `[0,1]` in the `{-1,0,1}` MZV alphabet.
pub fn zero_one_period(ctx: &Arc<PolyCtx>, word: &Word, table: &MzvReductionTable) -> Result<Rat> {
    zero_one_period_with_expansion(ctx, word, table, None)
}

pub fn zero_one_period_with_expansion(
    ctx: &Arc<PolyCtx>,
    word: &Word,
    table: &MzvReductionTable,
    expansion: Option<&MzvExpansionTable>,
) -> Result<Rat> {
    if word.is_empty() {
        return Ok(Rat::one(ctx.clone()));
    }
    if all_zero(word) {
        return Ok(Rat::zero(ctx.clone()));
    }

    if word[word.len() - 1].is_zero() {
        let seed = Wordlist::new(vec![WordlistTerm::new(Rat::one(ctx.clone()), word.clone())]);
        let regularized = reg0(&seed)?;
        let mut result = Rat::zero(ctx.clone());
        for term in regularized.terms {
            let period = zero_one_period_with_expansion(ctx, &term.word, table, expansion)?;
            result = result.try_add(&term.coef.try_mul(&period)?)?;
        }
        return Ok(result);
    }

    if word[0].to_string() == "1" {
        let seed = Wordlist::new(vec![WordlistTerm::new(Rat::one(ctx.clone()), word.clone())]);
        let regularized = reg_head(&seed, &Rat::one(ctx.clone()), &Rat::zero(ctx.clone()))?;
        let mut result = Rat::zero(ctx.clone());
        for term in regularized.terms {
            let period = zero_one_period_with_expansion(ctx, &term.word, table, expansion)?;
            result = result.try_add(&term.coef.try_mul(&period)?)?;
        }
        return Ok(result);
    }

    for letter in &word.letters {
        if !matches!(integer_letter(letter, "zero_one_period")?, -1..=1) {
            return Err(Error::InvalidInput(
                "zero_one_period: letter outside {-1,0,1}".into(),
            ));
        }
    }
    let seed = Wordlist::new(vec![WordlistTerm::new(Rat::one(ctx.clone()), word.clone())]);
    let raw = to_mzv_with_expansion(ctx, &seed, expansion)?;
    apply_mzv_reductions(table, &raw)
}

/// Evaluate a period over `[0,∞)` in HyperFLINT's `{-2,-1,0}` scope.
pub fn zero_inf_period(ctx: &Arc<PolyCtx>, word: &Word, table: &MzvReductionTable) -> Result<Rat> {
    zero_inf_period_with_expansion(ctx, word, table, None)
}

pub fn zero_inf_period_with_expansion(
    ctx: &Arc<PolyCtx>,
    word: &Word,
    table: &MzvReductionTable,
    expansion: Option<&MzvExpansionTable>,
) -> Result<Rat> {
    if word.is_empty() {
        return Ok(Rat::one(ctx.clone()));
    }
    if all_zero(word) {
        return Ok(Rat::zero(ctx.clone()));
    }

    let mut nonzero = Vec::new();
    let mut has_zero = false;
    for letter in &word.letters {
        if letter.is_zero() {
            has_zero = true;
        } else {
            nonzero.push(integer_letter(letter, "zero_inf_period")?);
        }
    }
    if !has_zero && !nonzero.is_empty() && nonzero.iter().all(|value| *value == -1) {
        return Ok(Rat::zero(ctx.clone()));
    }

    if word[word.len() - 1].is_zero() {
        let seed = Wordlist::new(vec![WordlistTerm::new(Rat::one(ctx.clone()), word.clone())]);
        let regularized = reg0(&seed)?;
        let mut result = Rat::zero(ctx.clone());
        for term in regularized.terms {
            let period = zero_inf_period_with_expansion(ctx, &term.word, table, expansion)?;
            result = result.try_add(&term.coef.try_mul(&period)?)?;
        }
        return Ok(result);
    }

    if nonzero.iter().all(|value| *value == -1) {
        let converted = convert_zero_one(&Wordlist::new(vec![WordlistTerm::new(
            Rat::one(ctx.clone()),
            word.clone(),
        )]))?;
        let mut result = Rat::zero(ctx.clone());
        for term in converted.terms {
            let period = zero_one_period_with_expansion(ctx, &term.word, table, expansion)?;
            result = result.try_add(&term.coef.try_mul(&period)?)?;
        }
        return Ok(result);
    }

    let unique = nonzero.iter().copied().collect::<BTreeSet<_>>();
    if unique.len() == 1 {
        let letter = *unique.first().expect("one distinct nonzero letter");
        if letter != -2 {
            return Err(Error::InvalidInput(format!(
                "zero_inf_period: single-letter rescaling supports -2, got {letter}"
            )));
        }
        let scaled = Word::new(
            word.letters
                .iter()
                .map(|value| {
                    if value.is_zero() {
                        Ok(Rat::zero(ctx.clone()))
                    } else {
                        Ok(Rat::from_int(
                            ctx.clone(),
                            integer_letter(value, "zero_inf_period")? / 2,
                        ))
                    }
                })
                .collect::<Result<Vec<_>>>()?,
        );
        let log2 = log_two_atom();
        let log2_index = ctx
            .index_of_indeterminate(log2.as_view())
            .ok_or_else(|| Error::UnknownVariable("Log2".into()))?;
        let negative_log2 = Rat::from_poly(Poly::generator(ctx.clone(), log2_index)?).negated();
        let mut log_factor = Rat::one(ctx.clone());
        let mut result = Rat::zero(ctx.clone());
        for offset in 0..=scaled.len() {
            let tail = Word::new(scaled.letters[offset..].to_vec());
            let period = zero_inf_period_with_expansion(ctx, &tail, table, expansion)?;
            result = result.try_add(&log_factor.try_mul(&period)?)?;
            if offset < scaled.len() {
                log_factor = log_factor
                    .try_mul(&negative_log2)?
                    .try_div(&Rat::from_int(ctx.clone(), (offset + 1) as i64))?;
            }
        }
        return Ok(result);
    }

    if unique == BTreeSet::from([-2, -1]) {
        let shifted = Word::new(
            word.letters
                .iter()
                .map(|letter| {
                    if letter.is_zero() {
                        Ok(Rat::one(ctx.clone()))
                    } else {
                        Ok(Rat::from_int(
                            ctx.clone(),
                            integer_letter(letter, "zero_inf_period")? + 1,
                        ))
                    }
                })
                .collect::<Result<Vec<_>>>()?,
        );
        let converted = convert_one_infinity_to_zero_one(&Wordlist::new(vec![WordlistTerm::new(
            Rat::one(ctx.clone()),
            shifted,
        )]))?;
        let mut result = Rat::zero(ctx.clone());
        for term in converted.terms {
            let period = zero_one_period_with_expansion(ctx, &term.word, table, expansion)?;
            result = result.try_add(&term.coef.try_mul(&period)?)?;
        }
        return Ok(result);
    }

    Err(Error::InvalidInput(
        "zero_inf_period: letters outside the {-2,-1,0} MZV scope".into(),
    ))
}

/// Absorb every evaluable regulator key into the constant term.
pub fn evaluate_periods(
    ctx: &Arc<PolyCtx>,
    regulator: &Regulator,
    table: &MzvReductionTable,
) -> Result<Regulator> {
    let mut constant = Rat::zero(ctx.clone());
    let mut passthrough = Regulator::new();
    for term in regulator {
        if term.key.is_empty() {
            constant = constant.try_add(&term.coef)?;
            continue;
        }

        let mut product = Rat::one(ctx.clone());
        let mut evaluable = true;
        for word in &term.key {
            match zero_inf_period(ctx, word, table) {
                Ok(period) => product = product.try_mul(&period)?,
                Err(_) => {
                    evaluable = false;
                    break;
                }
            }
        }
        if evaluable {
            constant = constant.try_add(&term.coef.try_mul(&product)?)?;
        } else {
            passthrough.push(term.clone());
        }
    }

    let mut output = Regulator::new();
    if !constant.is_zero() {
        output.push(RegTerm {
            coef: constant,
            key: RegKey::new(),
        });
    }
    output.extend(passthrough);
    canonicalize_regulator(&output)
}

/// A fibration-basis expansion with ordinary rational coefficients.
///
/// Each key slot corresponds to the variable at the same position in
/// [`Self::vars`]. An empty word is the multiplicative identity in that slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FibrationBasisResult {
    pub vars: Vec<String>,
    pub terms: Vec<(RegKey, Rat)>,
}

/// A fibration-basis expansion that can retain symbolic boundary residues.
///
/// If a terminal period cannot be evaluated by [`zero_inf_period`], its
/// regulator key is retained in the output in addition to the transformed
/// variable slots. This is the lossless counterpart to [`fibration_basis`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FibrationBasisResultSym {
    pub vars: Vec<String>,
    pub terms: Vec<(RegKey, SymCoef)>,
}

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

/// Project a rational regulator onto the ordered fibration basis.
///
/// Every requested variable is removed by [`transform_shuffle`], then all
/// terminal words are evaluated as `[0,∞)` periods. The operation fails if a
/// terminal period is outside the supported alphabet or a transform produces
/// a genuinely symbolic boundary residue.
pub fn fibration_basis(
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

/// Project a symbolic regulator onto the ordered fibration basis.
///
/// Unlike [`fibration_basis`], terminal periods outside the supported MZV
/// alphabet are retained as symbolic regulator-key factors instead of causing
/// the whole projection to fail.
pub fn fibration_basis_sym(
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

/// Test exact vanishing after projection onto the complete fibration basis.
pub fn test_zero_function_sym(
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

/// HyperFLINT's Rat-valued phase-6e zero-function residue.
///
/// As in the upstream API, distinct unevaluated keys are assumed independent;
/// the returned Rat is zero when their accumulated residue vanishes.
pub fn test_zero_function(
    ctx: &Arc<PolyCtx>,
    regulator: &Regulator,
    table: &MzvReductionTable,
) -> Result<Rat> {
    let reduced = evaluate_periods(ctx, regulator, table)?;
    let mut total = Rat::zero(ctx.clone());
    for term in reduced {
        total = total.try_add(&term.coef)?;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use symbolica::prelude::Symbol;

    use super::*;
    use crate::integrator::RegTermSym;
    use crate::reduce::{MzvReductionRule, build_mzv_atom_list};
    use crate::symbols::SYMBOL_NAMESPACE;

    fn setup() -> (Arc<PolyCtx>, MzvReductionTable) {
        let table = MzvReductionTable {
            reductions: vec![MzvReductionRule {
                lhs: "mzv_4".into(),
                rhs: "2/5*mzv_2^2".into(),
            }],
            basis: vec!["Log2".into(), "mzv_2".into(), "mzv_3".into()],
        };
        let x = Symbol::parse("x", SYMBOL_NAMESPACE).unwrap();
        let ctx = PolyCtx::from_indeterminates(build_mzv_atom_list(&table, [x.to_atom()]).unwrap())
            .unwrap();
        (ctx, table)
    }

    fn mzv(ctx: &Arc<PolyCtx>, indices: &[i64]) -> Rat {
        let atom = mzv_atom(indices);
        let index = ctx.index_of_indeterminate(atom.as_view()).unwrap();
        Rat::from_poly(Poly::generator(ctx.clone(), index).unwrap())
    }

    fn word(ctx: &Arc<PolyCtx>, letters: &[i64]) -> Word {
        Word::new(
            letters
                .iter()
                .map(|letter| Rat::from_int(ctx.clone(), *letter))
                .collect(),
        )
    }

    fn expression_word(ctx: &Arc<PolyCtx>, letters: &[&str]) -> Word {
        Word::new(
            letters
                .iter()
                .map(|letter| Rat::parse(ctx.clone(), letter).unwrap())
                .collect(),
        )
    }

    #[test]
    fn zero_one_period_mints_and_reduces_mzvs() {
        let (ctx, table) = setup();
        assert_eq!(
            zero_one_period(&ctx, &word(&ctx, &[0, 1]), &table).unwrap(),
            mzv(&ctx, &[2]).negated()
        );
        assert_eq!(
            zero_one_period(&ctx, &word(&ctx, &[0, 0, 0, 1]), &table).unwrap(),
            mzv(&ctx, &[2])
                .pow(2)
                .unwrap()
                .try_mul(&Rat::from_int(ctx.clone(), -2))
                .unwrap()
                .try_div(&Rat::from_int(ctx.clone(), 5))
                .unwrap()
        );
    }

    #[test]
    fn divergent_endpoint_words_are_regularized() {
        let (ctx, table) = setup();
        assert!(
            zero_one_period(&ctx, &word(&ctx, &[0, 0]), &table)
                .unwrap()
                .is_zero()
        );
        assert!(
            zero_inf_period(&ctx, &word(&ctx, &[-1, -1]), &table)
                .unwrap()
                .is_zero()
        );
    }

    #[test]
    fn evaluate_periods_keeps_parametric_keys_and_folds_constants() {
        let (ctx, table) = setup();
        let regulator = vec![
            RegTerm {
                coef: Rat::from_int(ctx.clone(), 2),
                key: vec![word(&ctx, &[0, 1])],
            },
            RegTerm {
                coef: Rat::one(ctx.clone()),
                key: vec![Word::new(vec![Rat::parse(ctx.clone(), "x").unwrap()])],
            },
        ];
        let result = evaluate_periods(&ctx, &regulator, &table).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(
            result[0].coef,
            mzv(&ctx, &[2])
                .try_mul(&Rat::from_int(ctx.clone(), -2))
                .unwrap()
        );
        assert!(result[0].key.is_empty());
    }

    #[test]
    fn fibration_basis_zero_variable_base_case_collects_constants() {
        let (ctx, table) = setup();
        let input = vec![
            RegTerm {
                coef: Rat::from_int(ctx.clone(), 7),
                key: RegKey::new(),
            },
            RegTerm {
                coef: Rat::from_int(ctx.clone(), -2),
                key: vec![Word::default()],
            },
        ];

        let result = fibration_basis(&ctx, &input, &[], &table).unwrap();
        assert!(result.vars.is_empty());
        assert_eq!(result.terms, vec![(RegKey::new(), Rat::from_int(ctx, 5))]);
    }

    #[test]
    fn fibration_basis_transforms_requested_variable_and_aggregates() {
        let (ctx, table) = setup();
        let x = ctx.index_of("x").unwrap();
        let variable_word = expression_word(&ctx, &["-x"]);
        let first = vec![
            RegTerm {
                coef: Rat::from_int(ctx.clone(), 2),
                key: vec![variable_word.clone()],
            },
            RegTerm {
                coef: Rat::from_int(ctx.clone(), 3),
                key: vec![variable_word.clone()],
            },
        ];
        let mut reversed = first.clone();
        reversed.reverse();

        let result = fibration_basis(&ctx, &first, &[x], &table).unwrap();
        let reversed_result = fibration_basis(&ctx, &reversed, &[x], &table).unwrap();
        assert_eq!(result, reversed_result);
        assert_eq!(result.vars, vec!["x"]);
        assert_eq!(result.terms.len(), 1);
        assert_eq!(result.terms[0].0, vec![word(&ctx, &[0])]);
        assert_eq!(result.terms[0].1, Rat::from_int(ctx, -5));
    }

    #[test]
    fn rational_fibration_rejects_an_unevaluable_terminal_period() {
        let (ctx, table) = setup();
        let input = vec![RegTerm {
            coef: Rat::one(ctx.clone()),
            key: vec![expression_word(&ctx, &["x"])],
        }];

        let error = fibration_basis(&ctx, &input, &[], &table).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("base-case period is not evaluable")
        );
    }

    #[test]
    fn symbolic_fibration_keeps_unevaluable_periods_and_tests_per_key() {
        let (ctx, table) = setup();
        let key = vec![expression_word(&ctx, &["x"])];
        let pi = SymCoef::pi_factor(ctx.clone());
        let input = vec![RegTermSym {
            coef: pi.clone(),
            key: key.clone(),
        }];

        let result = fibration_basis_sym(&ctx, &input, &[], &table).unwrap();
        assert!(result.vars.is_empty());
        assert_eq!(result.terms, vec![(key.clone(), pi.clone())]);
        assert!(!test_zero_function_sym(&ctx, &input, &[], &table).unwrap());

        let cancelling = vec![
            RegTermSym {
                coef: pi.clone(),
                key: key.clone(),
            },
            RegTermSym {
                coef: pi.negated(),
                key,
            },
        ];
        assert!(test_zero_function_sym(&ctx, &cancelling, &[], &table).unwrap());
        assert!(
            fibration_basis_sym(&ctx, &cancelling, &[], &table)
                .unwrap()
                .terms
                .is_empty()
        );
    }

    #[test]
    fn fibration_basis_validates_variable_indices_before_recursing() {
        let (ctx, table) = setup();
        let error = fibration_basis(&ctx, &Regulator::new(), &[ctx.len()], &table).unwrap_err();
        assert!(matches!(error, Error::UnknownVariable(_)));
    }
}
