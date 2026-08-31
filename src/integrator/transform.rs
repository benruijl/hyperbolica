use std::collections::HashMap;
use std::sync::Arc;

use crate::algebra::linear_factors::linear_factors;
use crate::algebra::shuffle::shuffle_product;
use crate::core::{PolyCtx, Rat, SymCoef};
use crate::error::{Error, Result};
use crate::symbols::{Word, Wordlist, WordlistTerm};

use super::regularize::regzero_word_in_ctx;

/// A commutative product of symbolic hyperlogarithm periods.
pub type RegKey = Vec<Word>;

/// One rational term in a symbolic regulator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegTerm {
    pub coef: Rat,
    pub key: RegKey,
}

pub type Regulator = Vec<RegTerm>;

/// Symbolic-coefficient variant used by the integration pipeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegTermSym {
    pub coef: SymCoef,
    pub key: RegKey,
}

pub type RegulatorSym = Vec<RegTermSym>;

/// A transformed word is a shuffle expression times a regulator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransformPair {
    pub shuffle: Wordlist,
    pub regulator: RegulatorSym,
}

pub type TransformResult = Vec<TransformPair>;

fn require_rat_context(value: &Rat, ctx: &PolyCtx) -> Result<()> {
    if value.ctx().vars() == ctx.vars() {
        Ok(())
    } else {
        Err(Error::ContextMismatch)
    }
}

fn require_word_context(word: &Word, ctx: &PolyCtx) -> Result<()> {
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

fn word_depends_on_variable(word: &Word, variable: usize) -> Result<bool> {
    for letter in &word.letters {
        if !letter.derivative(variable)?.is_zero() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn all_letters_equal_integer(word: &Word, value: i64) -> bool {
    if word.is_empty() {
        return false;
    }
    let expected = Rat::from_int(word[0].ctx().clone(), value);
    word.letters.iter().all(|letter| letter.equal(&expected))
}

fn one_regulator(ctx: &Arc<PolyCtx>, key: RegKey) -> RegulatorSym {
    vec![RegTermSym {
        coef: SymCoef::one(ctx.clone()),
        key: canonicalize_regkey(&key),
    }]
}

/// Compute the regularized limit at `variable = 0`.
///
/// Rational scaling and trailing-zero decomposition are evaluated exactly.
/// Periods that require the separate MZV/contour layer remain as symbolic
/// words in the regulator key, which is the shape-preserving HyperFLINT
/// phase-5 contract.
pub fn reglim_word(ctx: &Arc<PolyCtx>, word: &Word, variable: usize) -> Result<RegulatorSym> {
    if variable >= ctx.len() {
        return Err(Error::UnknownVariable(variable.to_string()));
    }
    require_word_context(word, ctx)?;
    reglim_word_impl(ctx, word, variable)
}

fn reglim_word_impl(ctx: &Arc<PolyCtx>, word: &Word, variable: usize) -> Result<RegulatorSym> {
    if word.is_empty() {
        return Ok(one_regulator(ctx, Vec::new()));
    }

    if !word_depends_on_variable(word, variable)? {
        if all_letters_equal_integer(word, 0) || all_letters_equal_integer(word, -1) {
            return Ok(Vec::new());
        }
        return Ok(one_regulator(ctx, vec![word.clone()]));
    }

    let zero_orders = word
        .letters
        .iter()
        .map(|letter| letter.pole_degree(variable))
        .collect::<Result<Vec<_>>>()?;
    let minimum_order = *zero_orders
        .iter()
        .min()
        .expect("a non-empty word has at least one Laurent order");
    let trailing = zero_orders
        .iter()
        .rev()
        .take_while(|order| **order > minimum_order)
        .count();

    if trailing > 0 {
        let head_length = word.len() - trailing;
        let head_base = Word::from(word.letters[..head_length].to_vec());
        let mut output = RegulatorSym::new();
        for appended_zeros in 0..=trailing {
            let mut head = head_base.clone();
            head.letters
                .extend((0..appended_zeros).map(|_| Rat::zero(ctx.clone())));
            let regularized_head = regzero_word_in_ctx(ctx, &head)?;
            let mut head_regulator = RegulatorSym::new();
            for term in regularized_head.terms {
                for subterm in reglim_word_impl(ctx, &term.word, variable)? {
                    head_regulator.push(RegTermSym {
                        coef: subterm.coef.try_mul_rat(&term.coef)?,
                        key: subterm.key,
                    });
                }
            }
            let head_regulator = canonicalize_regulator_sym(&head_regulator)?;

            let tail_start = head_length + appended_zeros;
            let tail = if tail_start < word.len() {
                Word::from(word.letters[tail_start..].to_vec())
            } else {
                Word::default()
            };
            let tail_regulator = reglim_word_impl(ctx, &tail, variable)?;
            output.extend(shuffle_symbolic_sym(&head_regulator, &tail_regulator)?);
        }
        return canonicalize_regulator_sym(&output);
    }

    let mut scaled = Word::default();
    for (letter, zero_order) in word.letters.iter().zip(zero_orders) {
        if zero_order > minimum_order {
            scaled.letters.push(Rat::zero(ctx.clone()));
        } else if letter.derivative(variable)?.is_zero() {
            scaled.letters.push(letter.clone());
        } else {
            scaled.letters.push(letter.residue(variable)?);
        }
    }

    if all_letters_equal_integer(&scaled, 0) || all_letters_equal_integer(&scaled, -1) {
        return Ok(Vec::new());
    }
    Ok(one_regulator(ctx, vec![scaled]))
}

fn trailing_zero(word: &Word) -> bool {
    word.letters.last().is_some_and(Rat::is_zero)
}

fn identity_transform(ctx: &Arc<PolyCtx>) -> TransformResult {
    vec![TransformPair {
        shuffle: Wordlist::from(vec![WordlistTerm::new(
            Rat::one(ctx.clone()),
            Word::default(),
        )]),
        regulator: one_regulator(ctx, Vec::new()),
    }]
}

struct ResultRow {
    regulator: RegulatorSym,
    word_indices: HashMap<String, usize>,
    terms: Vec<WordlistTerm>,
}

fn bump_result(
    rows: &mut Vec<ResultRow>,
    row_indices: &mut HashMap<String, usize>,
    regulator: &RegulatorSym,
    word: Word,
    coefficient: Rat,
) -> Result<()> {
    let regulator_key = regulator_sym_content_key(regulator)?;
    let row_index = if let Some(&index) = row_indices.get(&regulator_key) {
        index
    } else {
        let index = rows.len();
        row_indices.insert(regulator_key, index);
        rows.push(ResultRow {
            regulator: canonicalize_regulator_sym(regulator)?,
            word_indices: HashMap::new(),
            terms: Vec::new(),
        });
        index
    };

    let row = &mut rows[row_index];
    let word_key = word.content_key();
    if let Some(&index) = row.word_indices.get(&word_key) {
        row.terms[index].coef = row.terms[index].coef.try_add(&coefficient)?;
    } else {
        row.word_indices.insert(word_key, row.terms.len());
        row.terms.push(WordlistTerm::new(coefficient, word));
    }
    Ok(())
}

#[derive(Clone)]
struct SignedLinearFactor {
    multiplicity: i64,
    pole: Rat,
}

type TransformCache = HashMap<String, TransformResult>;

fn transform_cache_key(word: &Word, variable: usize) -> String {
    format!("{variable}|{}", word.content_key())
}

/// Transform one word into shuffle factors and regularized limits.
pub fn transform_word(ctx: &Arc<PolyCtx>, word: &Word, variable: usize) -> Result<TransformResult> {
    if variable >= ctx.len() {
        return Err(Error::UnknownVariable(variable.to_string()));
    }
    require_word_context(word, ctx)?;
    transform_word_impl(ctx, word, variable, &mut HashMap::new())
}

fn transform_word_impl(
    ctx: &Arc<PolyCtx>,
    word: &Word,
    variable: usize,
    cache: &mut TransformCache,
) -> Result<TransformResult> {
    let cache_key = transform_cache_key(word, variable);
    if let Some(cached) = cache.get(&cache_key) {
        return Ok(cached.clone());
    }
    if word.is_empty() {
        let output = identity_transform(ctx);
        cache.insert(cache_key, output.clone());
        return Ok(output);
    }

    let limit = reglim_word_impl(ctx, word, variable)?;
    let has_variable = word_depends_on_variable(word, variable)?;
    let mut rows = Vec::<ResultRow>::new();
    let mut row_indices = HashMap::<String, usize>::new();
    if !limit.is_empty() && has_variable {
        bump_result(
            &mut rows,
            &mut row_indices,
            &limit,
            Word::default(),
            Rat::one(ctx.clone()),
        )?;
    }

    if !has_variable {
        let output = if limit.is_empty() {
            Vec::new()
        } else {
            vec![TransformPair {
                shuffle: Wordlist::from(vec![WordlistTerm::new(
                    Rat::one(ctx.clone()),
                    Word::default(),
                )]),
                regulator: canonicalize_regulator_sym(&limit)?,
            }]
        };
        cache.insert(cache_key, output.clone());
        return Ok(output);
    }
    if trailing_zero(word) {
        return Err(Error::InvalidInput("TransformWord: $Failed".into()));
    }

    for index in 0..word.len() {
        if index == word.len() - 1 && word.len() >= 2 && word[word.len() - 2].is_zero() {
            break;
        }
        let difference = if index + 1 < word.len() {
            word[index].try_sub(&word[index + 1])?
        } else {
            word[index].clone()
        };
        if difference.is_zero() {
            continue;
        }

        let numerator = linear_factors(difference.numerator(), variable)?;
        let denominator = linear_factors(difference.denominator(), variable)?;
        let mut factors = Vec::<SignedLinearFactor>::new();
        for factor in numerator.linear {
            factors.push(SignedLinearFactor {
                multiplicity: i64::try_from(factor.multiplicity).map_err(|_| {
                    Error::InvalidInput("linear-factor multiplicity does not fit in i64".into())
                })?,
                pole: factor.pole,
            });
        }
        for factor in denominator.linear {
            factors.push(SignedLinearFactor {
                multiplicity: -i64::try_from(factor.multiplicity).map_err(|_| {
                    Error::InvalidInput("linear-factor multiplicity does not fit in i64".into())
                })?,
                pole: factor.pole,
            });
        }
        if factors.is_empty() {
            continue;
        }

        if index + 1 < word.len() {
            let mut letters = Vec::with_capacity(word.len() - 1);
            letters.extend(word.letters[..=index].iter().cloned());
            letters.extend(word.letters[index + 2..].iter().cloned());
            let subword = Word::from(letters);
            if !subword.is_empty() && !trailing_zero(&subword) {
                let transformed = transform_word_impl(ctx, &subword, variable, cache)?;
                append_factored_rows(&mut rows, &mut row_indices, &transformed, &factors, 1, ctx)?;
            }
        }

        let mut letters = Vec::with_capacity(word.len() - 1);
        letters.extend(word.letters[..index].iter().cloned());
        letters.extend(word.letters[index + 1..].iter().cloned());
        let subword = Word::from(letters);
        if subword.is_empty() || !trailing_zero(&subword) {
            let transformed = transform_word_impl(ctx, &subword, variable, cache)?;
            append_factored_rows(&mut rows, &mut row_indices, &transformed, &factors, -1, ctx)?;
        }
    }

    let output = rows
        .into_iter()
        .filter_map(|mut row| {
            row.terms.retain(|term| !term.coef.is_zero());
            (!row.terms.is_empty()).then_some(TransformPair {
                shuffle: Wordlist::from(row.terms),
                regulator: row.regulator,
            })
        })
        .collect::<Vec<_>>();
    cache.insert(cache_key, output.clone());
    Ok(output)
}

fn append_factored_rows(
    rows: &mut Vec<ResultRow>,
    row_indices: &mut HashMap<String, usize>,
    transformed: &TransformResult,
    factors: &[SignedLinearFactor],
    sign: i64,
    ctx: &Arc<PolyCtx>,
) -> Result<()> {
    for pair in transformed {
        for term in &pair.shuffle.terms {
            for factor in factors {
                let mut letters = Vec::with_capacity(term.word.len() + 1);
                letters.push(factor.pole.clone());
                letters.extend(term.word.letters.iter().cloned());
                let signed_multiplicity =
                    factor.multiplicity.checked_mul(sign).ok_or_else(|| {
                        Error::InvalidInput("signed linear-factor multiplicity overflowed".into())
                    })?;
                let coefficient =
                    Rat::from_int(ctx.clone(), signed_multiplicity).try_mul(&term.coef)?;
                bump_result(
                    rows,
                    row_indices,
                    &pair.regulator,
                    Word::from(letters),
                    coefficient,
                )?;
            }
        }
    }
    Ok(())
}

fn is_log_power(word: &Word) -> bool {
    let Some(first) = word.letters.first() else {
        return false;
    };
    word.letters.iter().all(|letter| letter.equal(first))
}

fn factorial_rat(ctx: &Arc<PolyCtx>, n: usize) -> Result<Rat> {
    let mut value = Rat::one(ctx.clone());
    for factor in 2..=n {
        let factor = i64::try_from(factor)
            .map_err(|_| Error::InvalidInput("word length does not fit in i64".into()))?;
        value = value.try_mul(&Rat::from_int(ctx.clone(), factor))?;
    }
    Ok(value)
}

fn scale_regulator(regulator: &RegulatorSym, scalar: &Rat) -> Result<RegulatorSym> {
    regulator
        .iter()
        .map(|term| {
            Ok(RegTermSym {
                coef: term.coef.try_mul_rat(scalar)?,
                key: term.key.clone(),
            })
        })
        .collect()
}

/// Transform a shuffle product of words.
pub fn transform_shuffle(
    ctx: &Arc<PolyCtx>,
    words: &[Word],
    variable: usize,
) -> Result<TransformResult> {
    if variable >= ctx.len() {
        return Err(Error::UnknownVariable(variable.to_string()));
    }
    for word in words {
        require_word_context(word, ctx)?;
    }
    if words.is_empty() {
        return Ok(identity_transform(ctx));
    }

    let mut letters = HashMap::<String, Rat>::new();
    let mut letter_order = Vec::<String>::new();
    let mut log_counts = HashMap::<String, usize>::new();
    let mut combined = Vec::<Word>::new();
    let mut combinatorial_factor = Rat::one(ctx.clone());
    let mut repeated = false;

    for word in words {
        if word.is_empty() {
            continue;
        }
        if is_log_power(word) {
            combinatorial_factor =
                combinatorial_factor.try_div(&factorial_rat(ctx, word.len())?)?;
            let key = word[0].to_string();
            if let Some(count) = log_counts.get_mut(&key) {
                *count = count.checked_add(word.len()).ok_or_else(|| {
                    Error::InvalidInput("combined logarithm depth overflowed usize".into())
                })?;
                repeated = true;
            } else {
                log_counts.insert(key.clone(), word.len());
                letters.insert(key.clone(), word[0].clone());
                letter_order.push(key);
            }
        } else {
            combined.push(word.clone());
        }
    }

    if repeated {
        for key in letter_order {
            let count = log_counts[&key];
            combined.push(Word::from(vec![letters[&key].clone(); count]));
        }
        for count in log_counts.values() {
            combinatorial_factor = combinatorial_factor.try_mul(&factorial_rat(ctx, *count)?)?;
        }
        let transformed = transform_shuffle(ctx, &combined, variable)?;
        return transformed
            .into_iter()
            .map(|pair| {
                Ok(TransformPair {
                    shuffle: pair.shuffle,
                    regulator: scale_regulator(&pair.regulator, &combinatorial_factor)?,
                })
            })
            .collect();
    }

    let mut accumulator = identity_transform(ctx);
    for word in words {
        let transformed = transform_word(ctx, word, variable)?;
        if transformed.is_empty() {
            return Ok(Vec::new());
        }
        let mut next = Vec::with_capacity(accumulator.len().saturating_mul(transformed.len()));
        for left in &accumulator {
            for right in &transformed {
                next.push(TransformPair {
                    shuffle: shuffle_product(&left.shuffle, &right.shuffle),
                    regulator: shuffle_symbolic_sym(&left.regulator, &right.regulator)?,
                });
            }
        }
        accumulator = next;
    }
    Ok(accumulator)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Arc<PolyCtx> {
        PolyCtx::new(["x", "y"]).unwrap()
    }

    fn rat(ctx: &Arc<PolyCtx>, expression: &str) -> Rat {
        Rat::parse(ctx.clone(), expression).unwrap()
    }

    fn word(ctx: &Arc<PolyCtx>, expressions: &[&str]) -> Word {
        Word::from(
            expressions
                .iter()
                .map(|expression| rat(ctx, expression))
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn canonical_keys_drop_identities_sort_and_collect() {
        let ctx = context();
        let first = word(&ctx, &["2"]);
        let second = word(&ctx, &["1"]);
        let regulator = vec![
            RegTerm {
                coef: rat(&ctx, "2"),
                key: vec![first.clone(), Word::default(), second.clone()],
            },
            RegTerm {
                coef: rat(&ctx, "-2"),
                key: vec![second, first],
            },
        ];
        assert!(collect_regulator(&regulator).unwrap().is_empty());
    }

    #[test]
    fn symbolic_shuffle_multiplies_coefficients_and_keys() {
        let ctx = context();
        let left = vec![RegTerm {
            coef: rat(&ctx, "x"),
            key: vec![word(&ctx, &["2"])],
        }];
        let right = vec![RegTerm {
            coef: rat(&ctx, "3"),
            key: vec![word(&ctx, &["1"])],
        }];
        let output = shuffle_symbolic(&left, &right).unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].coef, rat(&ctx, "3*x"));
        assert_eq!(output[0].key, vec![word(&ctx, &["1"]), word(&ctx, &["2"])]);
    }

    #[test]
    fn regularized_limit_handles_empty_constant_and_scaled_words() {
        let ctx = context();
        let empty = reglim_word(&ctx, &Word::default(), 0).unwrap();
        assert_eq!(empty, one_regulator(&ctx, Vec::new()));
        assert!(
            reglim_word(&ctx, &word(&ctx, &["0", "0"]), 0)
                .unwrap()
                .is_empty()
        );

        let constant = reglim_word(&ctx, &word(&ctx, &["2", "y"]), 0).unwrap();
        assert_eq!(constant, one_regulator(&ctx, vec![word(&ctx, &["2", "y"])]));

        let scaled = reglim_word(&ctx, &word(&ctx, &["x", "-x"]), 0).unwrap();
        assert_eq!(scaled, one_regulator(&ctx, vec![word(&ctx, &["1", "-1"])]));
    }

    #[test]
    fn transform_word_has_identity_constant_and_failed_tail_cases() {
        let ctx = context();
        assert_eq!(
            transform_word(&ctx, &Word::default(), 0).unwrap(),
            identity_transform(&ctx)
        );

        let constant = transform_word(&ctx, &word(&ctx, &["2"]), 0).unwrap();
        assert_eq!(constant.len(), 1);
        assert!(constant[0].shuffle.terms[0].word.is_empty());
        assert_eq!(
            constant[0].regulator,
            one_regulator(&ctx, vec![word(&ctx, &["2"])])
        );

        let failure = transform_word(&ctx, &word(&ctx, &["x", "0"]), 0).unwrap_err();
        assert!(failure.to_string().contains("$Failed"));
    }

    #[test]
    fn transform_variable_word_extracts_linear_poles() {
        let ctx = context();
        let transformed = transform_word(&ctx, &word(&ctx, &["-x"]), 0).unwrap();
        assert_eq!(transformed.len(), 1);
        assert!(transformed[0].regulator[0].key.is_empty());
        assert_eq!(transformed[0].shuffle.terms.len(), 1);
        assert_eq!(transformed[0].shuffle.terms[0].coef, rat(&ctx, "-1"));
        assert_eq!(transformed[0].shuffle.terms[0].word, word(&ctx, &["0"]));
    }

    #[test]
    fn transform_shuffle_collates_repeated_log_powers() {
        let ctx = context();
        let words = vec![word(&ctx, &["2"]), word(&ctx, &["2", "2"])];
        let transformed = transform_shuffle(&ctx, &words, 0).unwrap();
        assert_eq!(transformed.len(), 1);
        assert!(transformed[0].shuffle.terms[0].word.is_empty());
        assert_eq!(transformed[0].regulator.len(), 1);
        assert_eq!(
            transformed[0].regulator[0].coef.as_rat().unwrap(),
            rat(&ctx, "3")
        );
        assert_eq!(
            transformed[0].regulator[0].key,
            vec![word(&ctx, &["2", "2", "2"])]
        );
    }
}
