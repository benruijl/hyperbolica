use std::sync::Arc;

use crate::core::{Poly, PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::symbols::{Word, Wordlist, mzv_atom};

use crate::reduce::mzv_expansion::{ExpansionSource, MzvExpansionTable, cross_ctx_transfer_rat};

pub(super) fn integer_letter(letter: &Rat, site: &str) -> Result<i64> {
    letter
        .integer_constant()
        .and_then(|value| value.to_i64())
        .ok_or_else(|| Error::InvalidInput(format!("{site}: non-integer letter `{letter}`")))
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
    expansion: ExpansionSource<'_>,
) -> Result<Rat> {
    if word.is_empty() {
        return Ok(coefficient.clone());
    }
    if word[0].is_one() || word[word.len() - 1].is_zero() {
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
    } else if let Some(value) = expansion.lookup(&name)? {
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

pub(super) fn to_mzv(ctx: &Arc<PolyCtx>, wordlist: &Wordlist) -> Result<Rat> {
    to_mzv_with_expansion(ctx, wordlist, None)
}

pub(super) fn to_mzv_with_expansion(
    ctx: &Arc<PolyCtx>,
    wordlist: &Wordlist,
    expansion: Option<&MzvExpansionTable>,
) -> Result<Rat> {
    to_mzv_with_source(ctx, wordlist, expansion.into())
}

pub(super) fn to_mzv_with_source(
    ctx: &Arc<PolyCtx>,
    wordlist: &Wordlist,
    expansion: ExpansionSource<'_>,
) -> Result<Rat> {
    let mut result = Rat::zero(ctx.clone());
    for term in &wordlist.terms {
        result = result.try_add(&to_mzv_one_word(ctx, &term.coef, &term.word, expansion)?)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reduce::{
        MzvReductionRule, MzvReductionTable, apply_mzv_reductions, build_mzv_basis_atom_list,
        expand_mzv_reductions, standard_mzv_reductions, zero_one_period,
        zero_one_period_with_expansion,
    };
    use symbolica::prelude::{AtomCore, Symbol};

    fn zeta_word(ctx: &Arc<PolyCtx>, weight: usize) -> Word {
        let mut letters = vec![Rat::zero(ctx.clone()); weight - 1];
        letters.push(Rat::one(ctx.clone()));
        Word::new(letters)
    }

    fn custom_table() -> MzvReductionTable {
        MzvReductionTable::from_parts(
            vec![MzvReductionRule {
                lhs: "mzv_4".into(),
                rhs: "7*mzv_2".into(),
            }],
            vec!["mzv_2".into()],
        )
    }

    #[test]
    fn explicit_expansion_and_none_keep_their_original_meaning() {
        let table = standard_mzv_reductions();
        let ctx =
            PolyCtx::from_indeterminates(build_mzv_basis_atom_list(&table, []).unwrap()).unwrap();
        let word = zeta_word(&ctx, 4);
        assert!(
            matches!(zero_one_period_with_expansion(&ctx, &word, &table, None),
            Err(Error::UnknownVariable(message)) if message.contains("mzv_4"))
        );
        let custom = custom_table();
        let expansion = expand_mzv_reductions(&custom, false, "explicit test").unwrap();
        let explicit =
            zero_one_period_with_expansion(&ctx, &word, &table, Some(&expansion)).unwrap();
        let expected = Rat::from_atom(ctx.clone(), (mzv_atom(&[2]) * -7).as_view()).unwrap();
        assert_eq!(explicit, expected);
        assert_ne!(zero_one_period(&ctx, &word, &table).unwrap(), expected);
        assert!(zero_one_period(&ctx, &word, &custom).is_err());
    }

    #[test]
    fn native_context_slots_take_precedence_over_every_expansion_source() {
        let x = Symbol::parse("x", "period_precedence").unwrap().to_atom();
        let ctx =
            PolyCtx::from_indeterminates([mzv_atom(&[4]), mzv_atom(&[2]), x.clone()]).unwrap();
        let prefactor = Rat::from_atom(ctx.clone(), (x.clone() / (x + 1)).as_view()).unwrap();
        let input = Wordlist::new(vec![crate::symbols::WordlistTerm::new(
            prefactor.clone(),
            zeta_word(&ctx, 4),
        )]);
        let expansion = expand_mzv_reductions(&custom_table(), false, "precedence test").unwrap();
        let expected = prefactor
            .try_mul(&Rat::from_atom(ctx.clone(), mzv_atom(&[4]).as_view()).unwrap())
            .unwrap()
            .negated();
        for source in [
            ExpansionSource::None,
            ExpansionSource::Explicit(&expansion),
            ExpansionSource::Standard,
        ] {
            assert_eq!(to_mzv_with_source(&ctx, &input, source).unwrap(), expected);
        }
        let reduced = zero_one_period_with_expansion(
            &ctx,
            &zeta_word(&ctx, 4),
            &standard_mzv_reductions(),
            Some(&expansion),
        )
        .unwrap();
        let normal =
            zero_one_period(&ctx, &zeta_word(&ctx, 4), &standard_mzv_reductions()).unwrap();
        assert_eq!(reduced, normal);
    }

    #[test]
    fn standard_rule_index_never_uses_an_ordinary_symbols_spelling() {
        // Parse a user-supplied spelling; it is not a library-owned MZV head.
        let ordinary = symbolica::prelude::Atom::parse(
            "mzv_4",
            "period_user_symbol",
            symbolica::prelude::ParseSettings::default(),
        )
        .unwrap();
        let ctx = PolyCtx::from_indeterminates([ordinary.clone(), mzv_atom(&[4]), mzv_atom(&[2])])
            .unwrap();
        let input =
            Rat::from_atom(ctx.clone(), (ordinary.clone() + mzv_atom(&[4])).as_view()).unwrap();
        let expected = Rat::from_atom(
            ctx,
            (ordinary + mzv_atom(&[2]).pow(2) * symbolica::prelude::Atom::num((2, 5))).as_view(),
        )
        .unwrap();
        assert_eq!(
            apply_mzv_reductions(&standard_mzv_reductions(), &input).unwrap(),
            expected
        );
    }

    #[test]
    fn unknown_mzvs_do_not_create_new_lazy_entries_or_implicit_variables() {
        let table = standard_mzv_reductions();
        let ctx =
            PolyCtx::from_indeterminates(build_mzv_basis_atom_list(&table, []).unwrap()).unwrap();
        assert!(matches!(zero_one_period(&ctx, &zeta_word(&ctx, 9), &table),
            Err(Error::UnknownVariable(message)) if message.contains("mzv_9")));
    }
}
