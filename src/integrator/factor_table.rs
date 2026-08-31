//! Factor-prediction table for a fixed Fubini reduction chain.
//!
//! Pair and singleton objects are represented exactly as a rational constant
//! times signed powers of interned irreducible polynomials.  Trial division by
//! the stage pool is followed by a Symbolica factorization fallback; `oop`
//! records that fallback rather than weakening the exactness contract.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use symbolica::prelude::Rational;

use crate::core::Poly;
use crate::error::{Error, Result};

use super::lr_search::{ReductionEngine, lr_letter_admissible};
use super::structural_keys::PolyIndex;

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

fn intern(table: &mut FactorTable, polynomial: &Poly) -> usize {
    let canonical = polynomial.canonical_proportional_form();
    table
        .intern_index
        .insert(&mut table.intern_polys, canonical)
        .0
}

fn fold_constant(accumulator: &mut Poly, value: &Poly, sign: i64) -> Result<()> {
    if !value.is_rational_constant() {
        return Err(Error::InvalidInput(
            "factor-table constant fold received a non-constant".into(),
        ));
    }
    if sign > 0 {
        *accumulator = accumulator.try_mul(value)?;
    } else {
        *accumulator = accumulator.div_exact(value)?;
    }
    Ok(())
}

fn factor_into(
    table: &mut FactorTable,
    target: &Poly,
    pool: &[usize],
    exponents: &mut BTreeMap<usize, i64>,
    constant: &mut Poly,
    sign: i64,
) -> Result<bool> {
    let mut work = target.clone();
    let trial_started = Instant::now();
    for &id in pool {
        if work.is_rational_constant() {
            break;
        }
        let divisor = table.intern_polys[id].clone();
        while divisor.divides(&work)? {
            work = work.div_exact(&divisor)?;
            *exponents.entry(id).or_default() += sign;
            if work.is_rational_constant() {
                break;
            }
        }
    }
    table.stats.trial_seconds += trial_started.elapsed().as_secs_f64();
    if work.is_rational_constant() {
        fold_constant(constant, &work, sign)?;
        return Ok(false);
    }

    let fallback_started = Instant::now();
    let factorization = work.factor();
    for (base, multiplicity) in factorization.factors {
        let canonical = base.canonical_proportional_form();
        let id = intern(table, &canonical);
        for _ in 0..multiplicity {
            work = work.div_exact(&canonical)?;
        }
        *exponents.entry(id).or_default() += sign * multiplicity as i64;
    }
    table.stats.fallback_seconds += fallback_started.elapsed().as_secs_f64();
    if !work.is_rational_constant() {
        return Err(Error::InvalidInput(
            "factor-table fallback left a non-constant remainder".into(),
        ));
    }
    fold_constant(constant, &work, sign)?;
    Ok(true)
}

fn make_object(
    table: &mut FactorTable,
    numerators: &[Poly],
    denominators: &[Poly],
    pool: &[usize],
) -> Result<FactoredObject> {
    let Some(template) = numerators.first().or_else(|| denominators.first()) else {
        return Ok(FactoredObject::default());
    };
    if numerators.iter().any(Poly::is_zero) {
        return Ok(FactoredObject {
            constant: Rational::zero(),
            factors: Vec::new(),
            oop: false,
        });
    }
    let mut constant = Poly::one(template.ctx().clone());
    let mut exponents = BTreeMap::<usize, i64>::new();
    let mut oop = false;
    for numerator in numerators {
        if numerator.is_rational_constant() {
            fold_constant(&mut constant, numerator, 1)?;
        } else {
            oop |= factor_into(table, numerator, pool, &mut exponents, &mut constant, 1)?;
        }
    }
    for denominator in denominators {
        if denominator.is_zero() {
            return Err(Error::DivisionByZero);
        }
        if denominator.is_rational_constant() {
            fold_constant(&mut constant, denominator, -1)?;
        } else {
            oop |= factor_into(table, denominator, pool, &mut exponents, &mut constant, -1)?;
        }
    }
    if !constant.is_rational_constant() {
        return Err(Error::InvalidInput(
            "factor-table unit is not rational (internal error)".into(),
        ));
    }
    Ok(FactoredObject {
        constant: constant
            .rational_constant()
            .expect("factor-table unit was checked to be rational"),
        factors: exponents
            .into_iter()
            .filter(|(_, exponent)| *exponent != 0)
            .collect(),
        oop,
    })
}

fn max_degree(algebraic_letters: bool) -> Result<i64> {
    if !algebraic_letters {
        return Ok(1);
    }
    match std::env::var("HF_LR_MAX_DEG") {
        Ok(raw) if !raw.is_empty() => {
            let parsed = raw.parse::<i64>().map_err(|_| {
                Error::InvalidInput(format!("HF_LR_MAX_DEG must be an integer, got `{raw}`"))
            })?;
            Ok(parsed.max(1))
        }
        _ => Ok(2),
    }
}

/// Build a fixed-order factor-prediction table.
pub fn factor_table(
    group_polys: &[Vec<Poly>],
    order_indices: &[usize],
    algebraic_letters: bool,
    limits: FactorTableLimits,
) -> Result<FactorTable> {
    let mut unique = BTreeSet::new();
    if !order_indices
        .iter()
        .all(|variable| unique.insert(*variable))
    {
        return Err(Error::InvalidInput(
            "factor-table order indices must be unique".into(),
        ));
    }
    if let Some(reference) = group_polys.iter().flatten().next() {
        for &variable in order_indices {
            if variable >= reference.ctx().len() {
                return Err(Error::UnknownVariable(variable.to_string()));
            }
        }
        for polynomial in group_polys.iter().flatten() {
            if polynomial.ctx().vars() != reference.ctx().vars() {
                return Err(Error::ContextMismatch);
            }
        }
    }

    let maximum_degree = max_degree(algebraic_letters)?;
    let mut engine = ReductionEngine::new()?;
    let mut table = FactorTable::default();
    let mut pair_keys = BTreeSet::<(usize, usize, usize)>::new();
    let mut singleton_keys = BTreeSet::<(usize, usize)>::new();
    let mut current = group_polys.to_vec();

    for (stage_index, &variable) in order_indices.iter().enumerate() {
        engine.check_time("factor-table stage")?;
        let started = Instant::now();
        let forbidden_after = &order_indices[stage_index + 1..];
        let mut stage = StageInfo {
            var_idx: variable,
            admissible: Vec::with_capacity(current.len()),
            pool: Vec::new(),
            pair_count: 0,
            singleton_count: 0,
            inadmissible_count: 0,
            build_seconds: 0.0,
        };
        let mut next = vec![Vec::new(); current.len()];
        let mut stage_pool = BTreeSet::new();

        for group_index in 0..current.len() {
            let output = engine.reduce(&current[group_index], variable, None)?;
            let mut group_pool = Vec::new();
            let mut pool_seen = BTreeSet::new();
            for polynomial in output.iter().chain(&current[group_index]) {
                if polynomial.is_zero() || polynomial.is_rational_constant() {
                    continue;
                }
                let id = intern(&mut table, polynomial);
                if pool_seen.insert(id) {
                    group_pool.push(id);
                }
                stage_pool.insert(id);
            }

            let mut admissible = Vec::new();
            let mut linear = Vec::new();
            for polynomial in &current[group_index] {
                if polynomial.is_zero() || polynomial.is_rational_constant() {
                    continue;
                }
                let degree = polynomial.degree(variable)?;
                if degree < 1 {
                    continue;
                }
                if !lr_letter_admissible(
                    polynomial,
                    variable,
                    if algebraic_letters {
                        forbidden_after
                    } else {
                        &[]
                    },
                    maximum_degree,
                )? {
                    stage.inadmissible_count += 1;
                    continue;
                }
                let id = intern(&mut table, polynomial);
                admissible.push(id);
                if table.intern_polys[id].degree(variable)? == 1 {
                    linear.push(id);
                }
            }

            for &id in &admissible {
                if !singleton_keys.insert((variable, id)) {
                    continue;
                }
                table.stats.singletons_total += 1;
                if table.stats.singletons_total > limits.max_singletons {
                    return Err(Error::InvalidInput(format!(
                        "max_singletons exceeded (reached {})",
                        table.stats.singletons_total
                    )));
                }
                let letter = table.intern_polys[id].clone();
                let degree = letter.degree(variable)?;
                let mut coefficients = Vec::with_capacity((degree + 1) as usize);
                for power in (0..=degree).rev() {
                    let coefficient = letter.coefficient_of(variable, power)?;
                    let object = make_object(&mut table, &[coefficient], &[], &group_pool)?;
                    if object.oop {
                        table.stats.oop += 1;
                    }
                    coefficients.push(SingletonCoeff { power, object });
                }
                let discriminant = if degree == 2 {
                    engine.check_operand("factor-table discriminant", letter.n_terms())?;
                    let disc = letter.discriminant(variable)?;
                    let object = make_object(&mut table, &[disc], &[], &group_pool)?;
                    if object.oop {
                        table.stats.oop += 1;
                    }
                    Some(object)
                } else {
                    None
                };
                table.singletons.push(SingletonEntry {
                    var_idx: variable,
                    id,
                    degree,
                    coefficients,
                    discriminant,
                });
                stage.singleton_count += 1;
            }
            stage.admissible.push(admissible);

            for left_index in 0..linear.len() {
                for right_index in left_index + 1..linear.len() {
                    let mut left = linear[left_index];
                    let mut right = linear[right_index];
                    if left == right {
                        continue;
                    }
                    // Keep the internal pair key independent of encounter
                    // direction without formatting either polynomial. The
                    // JSON bridge restores HyperFLINT's historical lexical
                    // f/g orientation at the transport boundary.
                    if right < left {
                        std::mem::swap(&mut left, &mut right);
                    }
                    if !pair_keys.insert((variable, left, right)) {
                        continue;
                    }
                    table.stats.pairs_total += 1;
                    if table.stats.pairs_total > limits.max_pairs {
                        return Err(Error::InvalidInput(format!(
                            "max_pairs exceeded (reached {})",
                            table.stats.pairs_total
                        )));
                    }
                    let f = table.intern_polys[left].clone();
                    let g = table.intern_polys[right].clone();
                    let lc_f = f.coefficient_of(variable, 1)?;
                    let lc_g = g.coefficient_of(variable, 1)?;
                    let numerator = &(&lc_g * &f) - &(&lc_f * &g);
                    let difference =
                        make_object(&mut table, &[numerator], &[lc_f, lc_g], &group_pool)?;
                    if difference.oop {
                        table.stats.oop += 1;
                        table.stats.pair_fallbacks += 1;
                    }
                    table.pairs.push(PairEntry {
                        var_idx: variable,
                        f_id: left,
                        g_id: right,
                        difference,
                    });
                    stage.pair_count += 1;
                }
            }
            next[group_index] = output;
        }

        stage.pool.extend(stage_pool);
        stage.build_seconds = started.elapsed().as_secs_f64();
        table.stages.push(stage);
        current = next;
    }

    table
        .pairs
        .sort_by_key(|entry| (entry.var_idx, entry.f_id, entry.g_id));
    table
        .singletons
        .sort_by_key(|entry| (entry.var_idx, entry.id));
    Ok(table)
}

/// Naming-compatible alias for callers that prefer the upstream `build` API.
pub fn build(
    group_polys: &[Vec<Poly>],
    order_indices: &[usize],
    algebraic_letters: bool,
    limits: FactorTableLimits,
) -> Result<FactorTable> {
    factor_table(group_polys, order_indices, algebraic_letters, limits)
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
        let constant =
            Poly::parse(f.ctx().clone(), &entry.difference.constant.to_string()).unwrap();
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
        let table = factor_table(&groups, &[0, 1, 2], true, FactorTableLimits::default()).unwrap();
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
