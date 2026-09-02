use std::cmp::Ordering;
use std::fmt::Write;
use std::hash::Hash;

use super::{RegKey, RegTerm, RegTermSym, Regulator, RegulatorSym};
use crate::core::{
    DigestBuckets, PolyCtx, Rat, StableFnv1aHasher, SymCoef, structural_bucket_digest,
    structural_bucket_digest_by,
};
use crate::error::{Error, Result};
use crate::symbols::Word;

const REGKEY_BUCKET_DOMAIN: u64 = 0x5245_474b_4559_0001;
const REGULATOR_SYM_BUCKET_DOMAIN: u64 = 0x5245_4753_594d_0001;

fn same_context(left: &PolyCtx, right: &PolyCtx) -> bool {
    left.is_compatible_with(right)
}

fn require_rat_context(value: &Rat, ctx: &PolyCtx) -> Result<()> {
    if same_context(value.ctx(), ctx) {
        Ok(())
    } else {
        Err(Error::ContextMismatch)
    }
}

pub(super) fn require_word_context(word: &Word, ctx: &PolyCtx) -> Result<()> {
    for letter in &word.letters {
        require_rat_context(letter, ctx)?;
    }
    Ok(())
}

fn require_regkey_context(key: &RegKey, ctx: &PolyCtx) -> Result<()> {
    for word in key {
        require_word_context(word, ctx)?;
    }
    Ok(())
}

/// Drop identity words and sort the remaining factors deterministically.
pub fn canonicalize_regkey(key: &RegKey) -> RegKey {
    let mut canonical = key
        .iter()
        .filter(|word| !word.is_empty())
        .cloned()
        .collect::<Vec<_>>();
    // This is a semantic hot path. Canonicalize the commutative product with
    // the complete native order and do not allocate or format Atom strings.
    canonical.sort_unstable_by(Word::structural_cmp);
    canonical
}

/// Total structural order for regulator keys in their current word order.
pub(crate) fn regkey_structural_cmp(left: &RegKey, right: &RegKey) -> Ordering {
    for (left_word, right_word) in left.iter().zip(right) {
        let ordering = left_word.structural_cmp(right_word);
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}

/// Borrow regulator factors in legacy lexical presentation order.
///
/// Semantic canonicalization is structural. This helper is intentionally
/// restricted to final presentation and legacy serialization boundaries.
fn regkey_words_for_presentation(key: &[Word]) -> Vec<&Word> {
    let mut decorated = key
        .iter()
        .map(|word| (word.content_key(), word))
        .collect::<Vec<_>>();
    decorated.sort_unstable_by(|(left_key, left), (right_key, right)| {
        left_key
            .cmp(right_key)
            .then_with(|| left.structural_cmp(right))
    });
    decorated.into_iter().map(|(_, word)| word).collect()
}

pub(super) fn regkey_bucket_digest(key: &RegKey) -> u64 {
    structural_bucket_digest(REGKEY_BUCKET_DOMAIN, key)
}

fn hash_symcoef(value: &crate::core::SymCoef, state: &mut StableFnv1aHasher) {
    value.ctx().native_variables().hash(state);
    value.terms().len().hash(state);
    for monomial in value.terms() {
        monomial.prefactor.hash_canonical_payload(state);
        monomial.pi_power.hash(state);
        monomial.i_power.hash(state);
        monomial.log_powers.len().hash(state);
        for entry in &monomial.log_powers {
            entry.hash(state);
        }
        monomial.delta_powers.len().hash(state);
        for entry in &monomial.delta_powers {
            entry.hash(state);
        }
        monomial.period_powers.len().hash(state);
        for entry in &monomial.period_powers {
            entry.hash(state);
        }
    }
}

fn symcoef_structurally_equal(left: &crate::core::SymCoef, right: &crate::core::SymCoef) -> bool {
    same_context(left.ctx(), right.ctx()) && left.terms() == right.terms()
}

fn symcoef_native_content_key(value: &SymCoef) -> String {
    if value.is_zero() {
        return "0".into();
    }
    let mut output = String::new();
    for (index, monomial) in value.terms().iter().enumerate() {
        if index != 0 {
            output.push_str(" + ");
        }
        let _ = write!(output, "({})", monomial.prefactor.to_atom());
        match monomial.pi_power {
            0 => {}
            1 => output.push_str("*Pi"),
            exponent => {
                let _ = write!(output, "*Pi^{exponent}");
            }
        }
        match monomial.i_power {
            0 => {}
            1 => output.push_str("*I"),
            exponent => {
                let _ = write!(output, "*I^{exponent}");
            }
        }
        for (argument, exponent) in &monomial.log_powers {
            let _ = write!(output, "*Log[{argument}]");
            if *exponent != 1 {
                let _ = write!(output, "^{exponent}");
            }
        }
        for (variable, exponent) in &monomial.delta_powers {
            let atom = value
                .ctx()
                .variable_atom(*variable)
                .expect("canonical SymCoef contains a validated delta index");
            let _ = write!(output, "*delta[{atom}]");
            if *exponent != 1 {
                let _ = write!(output, "^{exponent}");
            }
        }
        for (period, exponent) in &monomial.period_powers {
            let _ = write!(output, "*Period[{period}]");
            if *exponent != 1 {
                let _ = write!(output, "^{exponent}");
            }
        }
    }
    output
}

pub(super) fn regulator_sym_bucket_digest(regulator: &RegulatorSym) -> u64 {
    structural_bucket_digest_by(REGULATOR_SYM_BUCKET_DOMAIN, |state| {
        regulator.len().hash(state);
        for term in regulator {
            hash_symcoef(&term.coef, state);
            term.key.hash(state);
        }
    })
}

pub(super) fn regulator_sym_structurally_equal(left: &RegulatorSym, right: &RegulatorSym) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            symcoef_structurally_equal(&left.coef, &right.coef) && left.key == right.key
        })
}

/// Legacy transport/presentation spelling for a canonical regulator key.
///
/// This is not semantic identity and may collide for equally printed
/// namespaced values. Lookup uses structural buckets, and ordering callers
/// must apply the internal `regkey_structural_cmp` comparator as the tie-break.
pub fn regkey_content_key(key: &RegKey) -> String {
    let mut output = String::new();
    for word in regkey_words_for_presentation(key) {
        output.push_str(&word.content_key());
        output.push('\u{2}');
    }
    output
}

/// Collect equal rational regulator keys, preserving first occurrence.
pub fn collect_regulator(regulator: &Regulator) -> Result<Regulator> {
    collect_regulator_with_digest(regulator, regkey_bucket_digest)
}

pub(super) fn collect_regulator_with_digest(
    regulator: &Regulator,
    mut digest_key: impl FnMut(&RegKey) -> u64,
) -> Result<Regulator> {
    let Some(first) = regulator.first() else {
        return Ok(Vec::new());
    };
    let ctx = first.coef.ctx().clone();
    let mut indices = DigestBuckets::default();
    let mut collected = Vec::<RegTerm>::new();

    for term in regulator {
        require_rat_context(&term.coef, &ctx)?;
        require_regkey_context(&term.key, &ctx)?;
        let key = canonicalize_regkey(&term.key);
        let digest = digest_key(&key);
        if let Some(index) = indices.find(digest, |index| {
            collected.get(index).is_some_and(|term| term.key == key)
        }) {
            collected[index].coef = collected[index].coef.try_add(&term.coef)?;
        } else {
            indices.insert(digest, collected.len());
            collected.push(RegTerm {
                coef: term.coef.clone(),
                key,
            });
        }
    }
    collected.retain(|term| !term.coef.is_zero());
    Ok(collected)
}

/// Collect equal symbolic regulator keys, preserving first occurrence.
pub fn collect_regulator_sym(regulator: &RegulatorSym) -> Result<RegulatorSym> {
    collect_regulator_sym_with_digest(regulator, regkey_bucket_digest)
}

pub(super) fn collect_regulator_sym_with_digest(
    regulator: &RegulatorSym,
    mut digest_key: impl FnMut(&RegKey) -> u64,
) -> Result<RegulatorSym> {
    let Some(first) = regulator.first() else {
        return Ok(Vec::new());
    };
    let ctx = first.coef.ctx().clone();
    let mut indices = DigestBuckets::default();
    let mut collected = Vec::<RegTermSym>::new();

    for term in regulator {
        if !same_context(term.coef.ctx(), &ctx) {
            return Err(Error::ContextMismatch);
        }
        require_regkey_context(&term.key, &ctx)?;
        let key = canonicalize_regkey(&term.key);
        let digest = digest_key(&key);
        if let Some(index) = indices.find(digest, |index| {
            collected.get(index).is_some_and(|term| term.key == key)
        }) {
            collected[index].coef = collected[index].coef.try_add(&term.coef)?;
        } else {
            indices.insert(digest, collected.len());
            collected.push(RegTermSym {
                coef: term.coef.clone(),
                key,
            });
        }
    }
    collected.retain(|term| !term.coef.is_zero());
    Ok(collected)
}

/// Multiply regulator monomials by joining and canonically sorting keys.
pub fn shuffle_symbolic(left: &Regulator, right: &Regulator) -> Result<Regulator> {
    let mut output = Vec::with_capacity(left.len().saturating_mul(right.len()));
    for left_term in left {
        for right_term in right {
            let mut key = Vec::with_capacity(left_term.key.len() + right_term.key.len());
            key.extend(left_term.key.iter().cloned());
            key.extend(right_term.key.iter().cloned());
            output.push(RegTerm {
                coef: left_term.coef.try_mul(&right_term.coef)?,
                key: canonicalize_regkey(&key),
            });
        }
    }
    collect_regulator(&output)
}

/// Symbolic-coefficient analog of [`shuffle_symbolic`].
pub fn shuffle_symbolic_sym(left: &RegulatorSym, right: &RegulatorSym) -> Result<RegulatorSym> {
    let mut output = Vec::with_capacity(left.len().saturating_mul(right.len()));
    for left_term in left {
        for right_term in right {
            let mut key = Vec::with_capacity(left_term.key.len() + right_term.key.len());
            key.extend(left_term.key.iter().cloned());
            key.extend(right_term.key.iter().cloned());
            output.push(RegTermSym {
                coef: left_term.coef.try_mul(&right_term.coef)?,
                key: canonicalize_regkey(&key),
            });
        }
    }
    collect_regulator_sym(&output)
}

pub fn canonicalize_regulator(regulator: &Regulator) -> Result<Regulator> {
    let canonical = collect_regulator(regulator)?;
    // Formatting remains the primary legacy output ordering. Structural
    // comparison resolves presentation collisions deterministically.
    let mut decorated = canonical
        .into_iter()
        .map(|term| (regkey_content_key(&term.key), term))
        .collect::<Vec<_>>();
    decorated.sort_unstable_by(|(left_key, left), (right_key, right)| {
        left_key
            .cmp(right_key)
            .then_with(|| regkey_structural_cmp(&left.key, &right.key))
    });
    Ok(decorated.into_iter().map(|(_, term)| term).collect())
}

pub fn canonicalize_regulator_sym(regulator: &RegulatorSym) -> Result<RegulatorSym> {
    let canonical = collect_regulator_sym(regulator)?;
    // Formatting remains the primary legacy output ordering. Structural
    // comparison resolves presentation collisions deterministically.
    let mut decorated = canonical
        .into_iter()
        .map(|term| (regkey_content_key(&term.key), term))
        .collect::<Vec<_>>();
    decorated.sort_unstable_by(|(left_key, left), (right_key, right)| {
        left_key
            .cmp(right_key)
            .then_with(|| regkey_structural_cmp(&left.key, &right.key))
    });
    Ok(decorated.into_iter().map(|(_, term)| term).collect())
}

/// Legacy canonical presentation string, never a cache identity.
pub fn regulator_content_key(regulator: &Regulator) -> Result<String> {
    let mut output = String::new();
    for term in canonicalize_regulator(regulator)? {
        output.push_str(&crate::symbols::plain_atom_string(term.coef.to_atom()));
        output.push('\u{3}');
        output.push_str(&regkey_content_key(&term.key));
        output.push('\u{4}');
    }
    Ok(output)
}

/// Legacy canonical presentation string, never a cache identity.
pub fn regulator_sym_content_key(regulator: &RegulatorSym) -> Result<String> {
    let mut output = String::new();
    for term in canonicalize_regulator_sym(regulator)? {
        output.push_str(&symcoef_native_content_key(&term.coef));
        output.push('\u{3}');
        output.push_str(&regkey_content_key(&term.key));
        output.push('\u{4}');
    }
    Ok(output)
}
