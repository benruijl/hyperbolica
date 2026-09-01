//! Factor-prediction table for a fixed Fubini reduction chain.
//!
//! Pair and singleton objects are represented exactly as a rational constant
//! times signed powers of interned irreducible polynomials. Trial division by
//! the stage pool is followed by a Symbolica factorization fallback; `oop`
//! records that fallback rather than weakening the exactness contract.

use symbolica::prelude::Rational;

use crate::core::Poly;

use super::structural_keys::PolyIndex;

mod build;
mod object;

pub use build::{build, factor_table};

#[derive(Clone, Debug)]
pub struct FactoredObject {
    pub constant: Rational,
    pub factors: Vec<(usize, i64)>,
    pub oop: bool,
}

impl Default for FactoredObject {
    fn default() -> Self {
        Self {
            constant: Rational::one(),
            factors: Vec::new(),
            oop: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PairEntry {
    pub var_idx: usize,
    pub f_id: usize,
    pub g_id: usize,
    pub difference: FactoredObject,
}

#[derive(Clone, Debug)]
pub struct SingletonCoeff {
    pub power: i64,
    pub object: FactoredObject,
}

#[derive(Clone, Debug)]
pub struct SingletonEntry {
    pub var_idx: usize,
    pub id: usize,
    pub degree: i64,
    pub coefficients: Vec<SingletonCoeff>,
    pub discriminant: Option<FactoredObject>,
}

#[derive(Clone, Debug)]
pub struct StageInfo {
    pub var_idx: usize,
    pub admissible: Vec<Vec<usize>>,
    pub pool: Vec<usize>,
    pub pair_count: usize,
    pub singleton_count: usize,
    pub inadmissible_count: usize,
    pub build_seconds: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct FactorTableLimits {
    pub max_pairs: usize,
    pub max_singletons: usize,
    /// Reserved for JSON serialization, where the bridge enforces it.
    pub max_response_mb: usize,
}

impl Default for FactorTableLimits {
    fn default() -> Self {
        Self {
            max_pairs: 2_000_000,
            max_singletons: 200_000,
            max_response_mb: 512,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FactorTableStats {
    pub pairs_total: usize,
    pub singletons_total: usize,
    pub oop: usize,
    pub pair_fallbacks: usize,
    pub trial_seconds: f64,
    pub fallback_seconds: f64,
}

#[derive(Clone, Debug, Default)]
pub struct FactorTable {
    pub intern_polys: Vec<Poly>,
    pub stages: Vec<StageInfo>,
    pub pairs: Vec<PairEntry>,
    pub singletons: Vec<SingletonEntry>,
    pub stats: FactorTableStats,
    intern_index: PolyIndex,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::core::PolyCtx;

    use super::*;

    fn parse(ctx: &Arc<PolyCtx>, expression: &str) -> Poly {
        Poly::parse(ctx.clone(), expression).unwrap()
    }

    fn product(table: &FactorTable, object: &FactoredObject, positive: bool) -> Poly {
        let ctx = table.intern_polys[0].ctx().clone();
        let mut output = Poly::one(ctx);
        for &(id, exponent) in &object.factors {
            if (exponent > 0) == positive {
                output = &output * &table.intern_polys[id].pow(exponent.unsigned_abs() as usize);
            }
        }
        output
    }

    fn assert_pair_contract(table: &FactorTable, entry: &PairEntry) {
        let f = &table.intern_polys[entry.f_id];
        let g = &table.intern_polys[entry.g_id];
        let lc_f = f.coefficient_of(entry.var_idx, 1).unwrap();
        let lc_g = g.coefficient_of(entry.var_idx, 1).unwrap();
        let numerator = &(&lc_g * f) - &(&lc_f * g);
        let numerator_product = product(table, &entry.difference, true);
        let denominator_product = product(table, &entry.difference, false);
        let constant = Poly::from_rational(f.ctx().clone(), entry.difference.constant.clone());
        assert_eq!(
            &numerator * &denominator_product,
            &(&(&constant * &numerator_product) * &lc_f) * &lc_g
        );
    }

    #[test]
    fn verification_fixture_has_exact_pair_factors() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let groups = vec![vec![parse(&ctx, "1+x*y"), parse(&ctx, "x+y")]];
        let table = factor_table(&groups, &[0, 1], false, FactorTableLimits::default()).unwrap();
        assert_eq!(table.stages.len(), 2);
        assert_eq!(table.stages[0].pair_count, 1);
        assert_eq!(table.stats.pair_fallbacks, 0);
        assert_pair_contract(&table, &table.pairs[0]);
        let factors = table.pairs[0]
            .difference
            .factors
            .iter()
            .map(|(id, exponent)| (table.intern_polys[*id].clone(), *exponent))
            .collect::<Vec<_>>();
        assert!(
            factors
                .iter()
                .any(|(p, e)| p == &parse(&ctx, "y-1").canonical_proportional_form() && *e == 1)
        );
        assert!(
            factors
                .iter()
                .any(|(p, e)| p == &parse(&ctx, "y+1").canonical_proportional_form() && *e == 1)
        );
        assert!(
            factors
                .iter()
                .any(|(p, e)| p == &parse(&ctx, "y").canonical_proportional_form() && *e == -1)
        );
    }

    #[test]
    fn algebraic_stage_rejects_future_dependent_quadratic() {
        let ctx = PolyCtx::new(["x", "z", "y"]).unwrap();
        let groups = vec![vec![
            parse(&ctx, "y*x^2+x+1"),
            parse(&ctx, "z*x^2+x+1"),
            parse(&ctx, "x+y"),
        ]];
        // `z` is a coefficient parameter, while `y` is a later integration
        // variable. Only the quadratic whose leading coefficient depends on
        // the latter must be rejected at the x stage.
        let table = factor_table(&groups, &[0, 2], true, FactorTableLimits::default()).unwrap();
        assert_eq!(table.stages[0].inadmissible_count, 1);
        assert_eq!(table.stages[0].pair_count, 0);
        assert!(table.singletons.iter().any(|entry| {
            entry.var_idx == 0 && entry.degree == 2 && entry.discriminant.is_some()
        }));
    }

    #[test]
    fn pair_limit_is_loud() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let groups = vec![vec![
            parse(&ctx, "x+y"),
            parse(&ctx, "x+2*y"),
            parse(&ctx, "x+3*y"),
        ]];
        let error = factor_table(
            &groups,
            &[0, 1],
            false,
            FactorTableLimits {
                max_pairs: 2,
                ..FactorTableLimits::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("max_pairs"));
    }

    #[test]
    fn structural_interning_and_pair_order_are_deterministic() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let groups = vec![vec![
            parse(&ctx, "x+y"),
            parse(&ctx, "2*x+2*y"),
            parse(&ctx, "x-y"),
        ]];
        let first = factor_table(&groups, &[0, 1], false, FactorTableLimits::default()).unwrap();
        let second = factor_table(&groups, &[0, 1], false, FactorTableLimits::default()).unwrap();

        assert_eq!(first.intern_polys, second.intern_polys);
        assert_eq!(
            first
                .pairs
                .iter()
                .map(|entry| (entry.var_idx, entry.f_id, entry.g_id))
                .collect::<Vec<_>>(),
            second
                .pairs
                .iter()
                .map(|entry| (entry.var_idx, entry.f_id, entry.g_id))
                .collect::<Vec<_>>()
        );
        let proportional = parse(&ctx, "x+y").canonical_proportional_form();
        assert_eq!(
            first
                .intern_polys
                .iter()
                .filter(|polynomial| **polynomial == proportional)
                .count(),
            1
        );
    }
}
