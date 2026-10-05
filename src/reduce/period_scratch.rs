//! Period-tuples scratch-ring minting.
//!
//! The integration context can stay slim (kinematic variables only) while
//! boundary periods are evaluated in a private MZV ring.  The resulting
//! MZV factors are moved into `SymMonomial::period_powers`, so the hot
//! rational-polynomial calculations never carry the hundreds of MZV
//! indeterminates used by the reduction table.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock, RwLock};

use symbolica::prelude::{Q, Rational};

use crate::core::{PolyCtx, Rat, SymCoef, SymMonomial, global_period_table};
use crate::error::{Error, Result};
use crate::symbols::Word;

use super::mzv_expansion::{ExpansionSource, StandardMzvExpansion, standard_mzv_expansion_lazy};
use super::mzv_reduce::MzvReductionTable;
use super::periods::{zero_inf_period_with_source, zero_one_period_with_source};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct PeriodKey {
    letters: Vec<i64>,
    zero_one: bool,
}

struct PeriodTerm {
    scalar: Rational,
    powers: BTreeMap<u32, i32>,
}

struct Scratch {
    expansion: &'static StandardMzvExpansion,
    period_ids: Vec<u32>,
    // On-demand standard expansion emits basis atoms. There are no remaining
    // substitutions to perform in this ring.
    no_reductions: MzvReductionTable,
    periods: RwLock<HashMap<PeriodKey, Arc<Vec<PeriodTerm>>>>,
}

// Retain frequently encountered short boundary words without allowing a
// long-running library process to accumulate an unbounded period cache.
const MAX_CACHED_PERIODS: usize = 1024;
const MAX_CACHED_WEIGHT: usize = 8;
const MAX_CACHED_TERMS: usize = 128;

/// Whether the supplied context is the slim, period-tuple representation.
///
/// A context containing any registered library constant is a legacy/wide
/// context.  This guard is deliberately structural rather than name-based so
/// user variables with ordinary spellings cannot accidentally enable tuples.
pub(crate) fn period_tuples_active(ctx: &Arc<PolyCtx>, table: &MzvReductionTable) -> bool {
    if !table.is_embedded_standard() {
        return false;
    }
    (0..ctx.len()).all(|index| {
        ctx.variable_atom(index)
            .map(|atom| !crate::symbols::is_library_constant(atom.as_view()))
            .unwrap_or(false)
    })
}

fn scratch(table: &MzvReductionTable) -> Result<&'static Scratch> {
    if !table.is_embedded_standard() {
        return Err(Error::InvalidInput(
            "period tuples require the embedded standard MZV table".into(),
        ));
    }

    static SCRATCH: OnceLock<std::result::Result<Scratch, String>> = OnceLock::new();
    SCRATCH
        .get_or_init(|| {
            (|| -> Result<Scratch> {
                let expansion = standard_mzv_expansion_lazy()?;
                let period_ids = expansion
                    .basis_names
                    .iter()
                    .map(|key| global_period_table().id_for(key.clone()))
                    .collect::<Result<Vec<_>>>()?;
                Ok(Scratch {
                    expansion,
                    period_ids,
                    no_reductions: MzvReductionTable::default(),
                    periods: RwLock::new(HashMap::new()),
                })
            })()
            .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|message| Error::InvalidInput(message.clone()))
}

/// Evaluate a numeric boundary word and represent its MZV content as period
/// powers over the caller's slim context.
pub(crate) fn mint_period_sym(
    slim: &Arc<PolyCtx>,
    word: &Word,
    table: &MzvReductionTable,
    zero_one: bool,
) -> Result<SymCoef> {
    if word.is_empty() {
        return Ok(SymCoef::one(slim.clone()));
    }
    if table.is_embedded_standard() && word.letters.iter().all(Rat::is_zero) {
        return Ok(SymCoef::zero(slim.clone()));
    }

    let scratch = scratch(table)?;
    let key = PeriodKey {
        letters: word
            .letters
            .iter()
            .map(|letter| {
                letter
                    .integer_constant()
                    .and_then(|value| value.to_i64())
                    .ok_or_else(|| {
                        Error::InvalidInput(format!(
                            "period tuple requires integer letters, got `{letter}`"
                        ))
                    })
            })
            .collect::<Result<Vec<_>>>()?,
        zero_one,
    };
    let cached = scratch
        .periods
        .read()
        .map_err(|_| Error::InvalidInput("period scratch cache is poisoned".into()))?
        .get(&key)
        .cloned();
    let terms = if let Some(terms) = cached {
        terms
    } else {
        let terms = Arc::new(evaluate_period(scratch, &key)?);
        let mut cache = scratch
            .periods
            .write()
            .map_err(|_| Error::InvalidInput("period scratch cache is poisoned".into()))?;
        if cache.len() < MAX_CACHED_PERIODS
            && key.letters.len() <= MAX_CACHED_WEIGHT
            && terms.len() <= MAX_CACHED_TERMS
        {
            cache.entry(key).or_insert_with(|| terms.clone());
        }
        terms
    };
    let monomials = terms
        .iter()
        .map(|term| {
            let mut monomial =
                SymMonomial::new(Rat::from_rational(slim.clone(), term.scalar.clone()));
            monomial.period_powers = term.powers.clone();
            monomial
        })
        .collect();
    Ok(SymCoef::from_monomials(slim.clone(), monomials))
}

fn evaluate_period(scratch: &Scratch, key: &PeriodKey) -> Result<Vec<PeriodTerm>> {
    let ctx = &scratch.expansion.basis_ctx;
    let word = Word::new(
        key.letters
            .iter()
            .map(|&letter| Rat::from_int(ctx.clone(), letter))
            .collect(),
    );
    let period = if key.zero_one {
        zero_one_period_with_source(
            ctx,
            &word,
            &scratch.no_reductions,
            ExpansionSource::Standard,
        )?
    } else {
        zero_inf_period_with_source(
            ctx,
            &word,
            &scratch.no_reductions,
            ExpansionSource::Standard,
        )?
    };
    let native = period.native();
    if !native.denominator.is_constant() {
        return Err(Error::InvalidInput(
            "period tuple denominator unexpectedly depends on an MZV variable".into(),
        ));
    }
    let denominator = &native.denominator.coefficients[0];
    native
        .numerator
        .exponents_iter()
        .zip(native.numerator.coefficients.iter())
        .map(|(exponents, coefficient)| {
            let powers = exponents
                .iter()
                .enumerate()
                .filter(|(_, exponent)| **exponent != 0)
                .map(|(variable, &exponent)| (scratch.period_ids[variable], i32::from(exponent)))
                .collect::<BTreeMap<_, _>>();
            Ok(PeriodTerm {
                scalar: Q.to_element(coefficient.clone(), denominator.clone(), true),
                powers,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::symcoef_to_atom;
    use crate::core::{PolyCtx, simplify_symcoef};
    use crate::reduce::{
        build_mzv_atom_list, standard_mzv_reductions, zero_inf_period, zero_one_period,
    };

    fn word(ctx: &Arc<PolyCtx>, letters: &[i64]) -> Word {
        Word::new(
            letters
                .iter()
                .map(|&letter| Rat::from_int(ctx.clone(), letter))
                .collect(),
        )
    }

    #[test]
    fn numeric_period_is_minted_outside_the_slim_context() {
        let table = standard_mzv_reductions();
        let slim = PolyCtx::new(["s12", "s23"]).unwrap();
        let scratch_word = Word::new(vec![Rat::from_int(slim.clone(), -2)]);
        let value = mint_period_sym(&slim, &scratch_word, &table, false).unwrap();
        assert!(!value.is_zero());
        assert!(
            value
                .terms()
                .iter()
                .any(|term| !term.period_powers.is_empty())
        );
        assert!(value.terms().iter().all(|term| {
            term.prefactor
                .numerator()
                .used_variable_indices()
                .is_empty()
                && term
                    .prefactor
                    .denominator()
                    .used_variable_indices()
                    .is_empty()
        }));
    }

    #[test]
    fn slim_detection_rejects_registered_constants() {
        let table = standard_mzv_reductions();
        let slim = PolyCtx::new(["s12"]).unwrap();
        assert!(period_tuples_active(&slim, &table));
        let wide = PolyCtx::new(["s12", "Log2"]).unwrap();
        assert!(!period_tuples_active(&wide, &table));
    }

    #[test]
    fn basis_scratch_matches_wide_reductions_for_numeric_words() {
        let table = standard_mzv_reductions();
        let slim = PolyCtx::new(["period_equivalence_x"]).unwrap();
        let wide =
            PolyCtx::from_indeterminates(build_mzv_atom_list(&table, std::iter::empty()).unwrap())
                .unwrap();
        assert_eq!(
            scratch(&table).unwrap().expansion.basis_ctx.len(),
            table.basis().len()
        );
        assert!(wide.len() > 100);
        for (zero_one, alphabet) in [(false, [-2, -1, 0]), (true, [-1, 0, 1])] {
            let mut words = vec![Vec::new()];
            let mut layer = vec![Vec::new()];
            for _ in 0..3 {
                layer = layer
                    .into_iter()
                    .flat_map(|prefix| {
                        alphabet.map(|letter| {
                            let mut letters = prefix.clone();
                            letters.push(letter);
                            letters
                        })
                    })
                    .collect();
                words.extend(layer.iter().cloned());
            }
            words.extend(if zero_one {
                vec![vec![0, 0, 0, 1], vec![0, -1, 0, 1], vec![1, -1, 0, -1]]
            } else {
                vec![vec![0, 0, 0, -1], vec![-2, 0, -1, -2], vec![0, -1, 0, -1]]
            });
            for letters in words {
                let tuple =
                    mint_period_sym(&slim, &word(&slim, &letters), &table, zero_one).unwrap();
                let restored =
                    Rat::from_atom(wide.clone(), symcoef_to_atom(&tuple).unwrap().as_view())
                        .unwrap();
                let original = if zero_one {
                    zero_one_period(&wide, &word(&wide, &letters), &table)
                } else {
                    zero_inf_period(&wide, &word(&wide, &letters), &table)
                }
                .unwrap();
                assert_eq!(
                    restored, original,
                    "letters={letters:?}, zero_one={zero_one}"
                );
            }
        }
    }

    #[test]
    fn minted_zeta_two_and_pi_squared_cancel_exactly() {
        let table = standard_mzv_reductions();
        let ctx = PolyCtx::new(["period_pi_cancellation_x"]).unwrap();
        let period = mint_period_sym(&ctx, &word(&ctx, &[0, 1]), &table, true).unwrap();
        let mut pi_squared_over_six = SymMonomial::new(
            Rat::one(ctx.clone())
                .try_div(&Rat::from_int(ctx.clone(), 6))
                .unwrap(),
        );
        pi_squared_over_six.pi_power = 2;
        let sum = period
            .try_add(&SymCoef::from_monomials(ctx, vec![pi_squared_over_six]))
            .unwrap();
        assert!(simplify_symcoef(&sum, &table).unwrap().is_zero());
    }

    #[test]
    fn cached_periods_do_not_retain_the_callers_context() {
        let table = standard_mzv_reductions();
        let left = PolyCtx::new(["period_cache_left"]).unwrap();
        let right = PolyCtx::new(["period_cache_right", "period_cache_other"]).unwrap();
        let first = mint_period_sym(&left, &word(&left, &[-2, -1]), &table, false).unwrap();
        let second = mint_period_sym(&right, &word(&right, &[-2, -1]), &table, false).unwrap();
        assert_eq!(
            symcoef_to_atom(&first).unwrap(),
            symcoef_to_atom(&second).unwrap()
        );
        assert!(
            second
                .terms()
                .iter()
                .all(|term| term.prefactor.ctx().is_compatible_with(&right))
        );
    }

    #[test]
    fn unsupported_letters_and_custom_tables_are_rejected() {
        let ctx = PolyCtx::new(["period_invalid_x"]).unwrap();
        let table = standard_mzv_reductions();
        assert!(mint_period_sym(&ctx, &word(&ctx, &[-3]), &table, false).is_err());
        assert!(mint_period_sym(&ctx, &word(&ctx, &[2]), &table, true).is_err());
        assert!(
            mint_period_sym(
                &ctx,
                &word(&ctx, &[-1]),
                &MzvReductionTable::default(),
                false
            )
            .is_err()
        );
    }
}
