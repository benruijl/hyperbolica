//! Demand-driven access to the fixed, flat standard MZV table.
//!
//! Public eager/custom-table expansion remains independent. Only the embedded
//! standard table uses these bounded per-rule caches.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use crate::core::{PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::reduce::mzv_reduce::{
    MzvReductionTable, mzv_constant_atom, mzv_expression_atom, standard_mzv_reductions,
    standard_mzv_rule_index,
};

use super::{MzvExpansionTable, canonical_basis};

/// Internal choice of expansion behavior. An explicit `None` must not acquire
/// standard-table semantics merely because an evaluator also has a table.
#[derive(Clone, Copy)]
pub(crate) enum ExpansionSource<'a> {
    None,
    Explicit(&'a MzvExpansionTable),
    Standard,
}

impl<'a> From<Option<&'a MzvExpansionTable>> for ExpansionSource<'a> {
    fn from(value: Option<&'a MzvExpansionTable>) -> Self {
        value.map_or(Self::None, Self::Explicit)
    }
}

impl<'a> ExpansionSource<'a> {
    pub(crate) fn lookup(self, name: &str) -> Result<Option<&'a Rat>> {
        match self {
            Self::None => Ok(None),
            Self::Explicit(expansion) => Ok(expansion.expansion.get(name)),
            Self::Standard => standard_mzv_expansion_lazy()?.lookup(name),
        }
    }
}

pub(crate) struct StandardMzvExpansion {
    pub(crate) basis_ctx: Arc<PolyCtx>,
    pub(crate) basis_names: Vec<String>,
    table: MzvReductionTable,
    rule_index: &'static HashMap<String, usize>,
    // Exactly one slot per immutable generated rule: no request-specific
    // contexts or caller data are retained in this process-wide cache.
    values: Vec<OnceLock<std::result::Result<Rat, String>>>,
}

impl StandardMzvExpansion {
    fn new() -> Result<Self> {
        let table = standard_mzv_reductions();
        let basis_names = canonical_basis(table.basis())?;
        let basis_atoms = basis_names
            .iter()
            .map(|name| {
                mzv_constant_atom(name).ok_or_else(|| {
                    Error::InvalidInput(format!("invalid standard MZV basis identifier `{name}`"))
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let basis_ctx = PolyCtx::from_indeterminates(basis_atoms)?;
        let rule_index = standard_mzv_rule_index();
        if basis_names.iter().any(|name| rule_index.contains_key(name)) {
            return Err(Error::InvalidInput(
                "standard MZV basis contains a reducible atom".into(),
            ));
        }
        let values = (0..table.reductions().len())
            .map(|_| OnceLock::new())
            .collect();
        Ok(Self {
            basis_ctx,
            basis_names,
            table,
            rule_index,
            values,
        })
    }

    pub(crate) fn lookup(&self, name: &str) -> Result<Option<&Rat>> {
        let Some(&index) = self.rule_index.get(name) else {
            return Ok(None);
        };
        self.values[index]
            .get_or_init(|| {
                let rule = &self.table.reductions()[index];
                // The embedded table is flat. Conversion into this fixed
                // basis context rejects unknown or chained RHS atoms. The
                // exhaustive oracle test verifies every generated rule.
                mzv_expression_atom(&rule.rhs)
                    .and_then(|atom| Rat::from_atom(self.basis_ctx.clone(), atom.as_view()))
                    .map_err(|error| error.to_string())
            })
            .as_ref()
            .map(Some)
            .map_err(|message| {
                Error::InvalidInput(format!("cannot expand standard MZV `{name}`: {message}"))
            })
    }

    #[cfg(test)]
    fn initialized_rules(&self) -> usize {
        self.values
            .iter()
            .filter(|value| value.get().is_some())
            .count()
    }
}

static STANDARD_LAZY: OnceLock<std::result::Result<StandardMzvExpansion, String>> = OnceLock::new();

pub(crate) fn standard_mzv_expansion_lazy() -> Result<&'static StandardMzvExpansion> {
    STANDARD_LAZY
        .get_or_init(|| StandardMzvExpansion::new().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|message| {
            Error::InvalidInput(format!("cannot initialize standard MZV basis: {message}"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reduce::{
        build_mzv_basis_atom_list, standard_mzv_expansion, zero_inf_period, zero_one_period,
    };
    use crate::symbols::Word;

    #[test]
    fn every_lazy_standard_rule_equals_the_public_eager_oracle() {
        let lazy = StandardMzvExpansion::new().unwrap();
        assert_eq!(lazy.initialized_rules(), 0);
        assert_eq!(lazy.basis_ctx.len(), 10);
        assert_eq!(lazy.values.len(), 700);
        let eager = standard_mzv_expansion().unwrap();
        for (name, expected) in &eager.expansion {
            assert_eq!(lazy.lookup(name).unwrap().unwrap(), expected, "{name}");
        }
        assert_eq!(lazy.initialized_rules(), 700);
    }

    #[test]
    fn rule_lookup_is_bounded_and_only_initializes_requested_values() {
        let lazy = StandardMzvExpansion::new().unwrap();
        assert!(lazy.lookup("mzv_2").unwrap().is_none());
        assert!(lazy.lookup("mzv_999").unwrap().is_none());
        assert_eq!(lazy.initialized_rules(), 0);
        let first = lazy.lookup("mzv_4").unwrap().unwrap();
        let second = lazy.lookup("mzv_4").unwrap().unwrap();
        assert!(std::ptr::eq(first, second));
        assert_eq!(lazy.initialized_rules(), 1);
    }

    #[test]
    fn trivial_periods_do_not_initialize_the_standard_provider() {
        const CHILD: &str = "HYPERBOLICA_TEST_COLD_PERIOD_PROVIDER";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "reduce::mzv_expansion::lazy::tests::trivial_periods_do_not_initialize_the_standard_provider"])
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        assert!(STANDARD_LAZY.get().is_none());
        let ctx = PolyCtx::new(["cold_period_x"]).unwrap();
        let table = standard_mzv_reductions();
        for letters in [vec![], vec![0], vec![0, 0, 0]] {
            let word = Word::new(
                letters
                    .iter()
                    .map(|&letter| Rat::from_int(ctx.clone(), letter))
                    .collect(),
            );
            let expected = if letters.is_empty() {
                Rat::one(ctx.clone())
            } else {
                Rat::zero(ctx.clone())
            };
            assert_eq!(zero_one_period(&ctx, &word, &table).unwrap(), expected);
            assert_eq!(zero_inf_period(&ctx, &word, &table).unwrap(), expected);
        }
        let word = Word::new(vec![Rat::from_int(ctx.clone(), -1); 3]);
        assert!(zero_inf_period(&ctx, &word, &table).unwrap().is_zero());
        let zero_word = Word::new(vec![Rat::zero(ctx.clone())]);
        assert!(
            crate::reduce::period_scratch::mint_period_sym(&ctx, &zero_word, &table, false)
                .unwrap()
                .is_zero()
        );
        assert!(STANDARD_LAZY.get().is_none());
        assert!(super::super::STANDARD_MZV_EXPANSION.get().is_none());

        let basis =
            PolyCtx::from_indeterminates(build_mzv_basis_atom_list(&table, []).unwrap()).unwrap();
        let zeta_two = Word::new(vec![Rat::zero(basis.clone()), Rat::one(basis.clone())]);
        zero_one_period(&basis, &zeta_two, &table).unwrap();
        assert!(STANDARD_LAZY.get().is_none());

        let slim_zeta_two = Word::new(vec![Rat::zero(ctx.clone()), Rat::one(ctx.clone())]);
        crate::reduce::period_scratch::mint_period_sym(&ctx, &slim_zeta_two, &table, true).unwrap();
        assert_eq!(
            standard_mzv_expansion_lazy().unwrap().initialized_rules(),
            0
        );

        let zeta_four = Word::new(vec![
            Rat::zero(basis.clone()),
            Rat::zero(basis.clone()),
            Rat::zero(basis.clone()),
            Rat::one(basis.clone()),
        ]);
        zero_one_period(&basis, &zeta_four, &table).unwrap();
        assert_eq!(
            standard_mzv_expansion_lazy().unwrap().initialized_rules(),
            1
        );
        zero_one_period(&basis, &zeta_four, &table).unwrap();
        assert_eq!(
            standard_mzv_expansion_lazy().unwrap().initialized_rules(),
            1
        );
        assert!(super::super::STANDARD_MZV_EXPANSION.get().is_none());
    }
}
