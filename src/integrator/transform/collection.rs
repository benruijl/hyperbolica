use std::collections::HashMap;

use super::{RegKey, RegTerm, RegTermSym, Regulator, RegulatorSym};
use crate::core::{PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::symbols::Word;

fn require_rat_context(value: &Rat, ctx: &PolyCtx) -> Result<()> {
    if value.ctx().vars() == ctx.vars() {
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
    canonical.sort_by_key(Word::content_key);
    canonical
}

/// Collision-resistant transport key for a canonical regulator key.
pub fn regkey_content_key(key: &RegKey) -> String {
    let mut output = String::new();
    for word in key {
        output.push_str(&word.content_key());
        output.push('\u{2}');
    }
    output
}

/// Collect equal rational regulator keys, preserving first occurrence.
pub fn collect_regulator(regulator: &Regulator) -> Result<Regulator> {
    let Some(first) = regulator.first() else {
        return Ok(Vec::new());
    };
    let ctx = first.coef.ctx().clone();
    let mut indices = HashMap::<String, usize>::new();
    let mut collected = Vec::<RegTerm>::new();

    for term in regulator {
        require_rat_context(&term.coef, &ctx)?;
        require_regkey_context(&term.key, &ctx)?;
        let key = canonicalize_regkey(&term.key);
        let content_key = regkey_content_key(&key);
        if let Some(&index) = indices.get(&content_key) {
            collected[index].coef = collected[index].coef.try_add(&term.coef)?;
        } else {
            indices.insert(content_key, collected.len());
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
    let Some(first) = regulator.first() else {
        return Ok(Vec::new());
    };
    let ctx = first.coef.ctx().clone();
    let mut indices = HashMap::<String, usize>::new();
    let mut collected = Vec::<RegTermSym>::new();

    for term in regulator {
        if term.coef.ctx().vars() != ctx.vars() {
            return Err(Error::ContextMismatch);
        }
        require_regkey_context(&term.key, &ctx)?;
        let key = canonicalize_regkey(&term.key);
        let content_key = regkey_content_key(&key);
        if let Some(&index) = indices.get(&content_key) {
            collected[index].coef = collected[index].coef.try_add(&term.coef)?;
        } else {
            indices.insert(content_key, collected.len());
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
    let mut canonical = collect_regulator(regulator)?;
    canonical.sort_by_key(|term| regkey_content_key(&term.key));
    Ok(canonical)
}

pub fn canonicalize_regulator_sym(regulator: &RegulatorSym) -> Result<RegulatorSym> {
    let mut canonical = collect_regulator_sym(regulator)?;
    canonical.sort_by_key(|term| regkey_content_key(&term.key));
    Ok(canonical)
}

pub fn regulator_content_key(regulator: &Regulator) -> Result<String> {
    let mut output = String::new();
    for term in canonicalize_regulator(regulator)? {
        output.push_str(&term.coef.to_string());
        output.push('\u{3}');
        output.push_str(&regkey_content_key(&term.key));
        output.push('\u{4}');
    }
    Ok(output)
}

pub fn regulator_sym_content_key(regulator: &RegulatorSym) -> Result<String> {
    let mut output = String::new();
    for term in canonicalize_regulator_sym(regulator)? {
        output.push_str(&term.coef.to_string());
        output.push('\u{3}');
        output.push_str(&regkey_content_key(&term.key));
        output.push('\u{4}');
    }
    Ok(output)
}
