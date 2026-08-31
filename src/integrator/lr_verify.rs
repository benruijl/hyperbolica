//! Certification of one prescribed linear-reducibility order.
//!
//! A direct reduction chain is a useful fast accept, but not an authoritative
//! rejection: pairwise Fubini resultants can contain order-dependent spurious
//! letters.  When that screen finds a blocker, this module rebuilds the same
//! intersection-refined subset table as the order search and checks the
//! prescribed prefixes against that table.

use std::collections::{HashMap, HashSet};

use crate::core::Poly;
use crate::error::{Error, Result};

use super::lr_search::{ReductionEngine, intersect_proportional};

/// Result of certifying one prescribed integration order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderVerifyResult {
    /// Whether the prescribed order is linearly reducible.
    pub is_lr: bool,
    /// Zero-based integration step containing the first genuine blocker.
    pub blocking_step: i64,
    /// Degree of the blocking letter in that step's pivot.
    pub blocking_degree: i64,
    /// Whether a quadratic was rejected because it depends on a later pivot.
    pub forbidden_dep: bool,
    /// Canonical proportional representative of the blocking letter.
    ///
    /// This remains an exact structural polynomial throughout verification;
    /// transport adapters choose its external spelling at their boundary.
    pub blocking_letter: Option<Poly>,
    /// Whether `order_var_indices` is not a permutation of `xvar_indices`.
    pub malformed: bool,
}

impl Default for OrderVerifyResult {
    fn default() -> Self {
        Self {
            is_lr: false,
            blocking_step: -1,
            blocking_degree: 0,
            forbidden_dep: false,
            blocking_letter: None,
            malformed: false,
        }
    }
}

impl OrderVerifyResult {
    fn malformed() -> Self {
        Self {
            malformed: true,
            ..Self::default()
        }
    }

    fn accepted() -> Self {
        Self {
            is_lr: true,
            ..Self::default()
        }
    }
}

fn configured_max_degree(allow_algebraic_letters: bool) -> Result<i64> {
    if !allow_algebraic_letters {
        return Ok(1);
    }
    match std::env::var("HF_LR_MAX_DEG") {
        Ok(raw) if !raw.is_empty() => {
            let degree = raw.parse::<i64>().map_err(|_| {
                Error::InvalidInput(format!("HF_LR_MAX_DEG must be an integer, got `{raw}`"))
            })?;
            if degree < 1 {
                return Err(Error::InvalidInput(
                    "HF_LR_MAX_DEG must be at least one".into(),
                ));
            }
            Ok(degree)
        }
        _ => Ok(2),
    }
}

fn validate_contexts(groups: &[Vec<Poly>], variables: &[usize]) -> Result<()> {
    let Some(reference) = groups.iter().flatten().next() else {
        return Ok(());
    };
    for &variable in variables {
        if variable >= reference.ctx().len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
    }
    for polynomial in groups.iter().flatten() {
        if polynomial.ctx().vars() != reference.ctx().vars() {
            return Err(Error::ContextMismatch);
        }
    }
    Ok(())
}

fn subsets_of_size(n: usize, size: usize) -> Vec<u64> {
    if size > n {
        return Vec::new();
    }
    if size == 0 {
        return vec![0];
    }
    let mut output = Vec::new();
    let mut mask = (1_u64 << size) - 1;
    let limit = 1_u64 << n;
    while mask < limit {
        output.push(mask);
        let low = mask & mask.wrapping_neg();
        let next = mask + low;
        mask = (((next ^ mask) >> 2) / low) | next;
    }
    output
}

fn first_blocker(
    groups: &[Vec<Poly>],
    pivot: usize,
    step: usize,
    order: &[usize],
    max_degree: i64,
    allow_algebraic_letters: bool,
) -> Result<Option<OrderVerifyResult>> {
    for group in groups {
        for letter in group {
            let degree = letter.degree(pivot)?;
            if degree < 1 {
                continue;
            }
            if degree > max_degree {
                return Ok(Some(OrderVerifyResult {
                    blocking_step: step as i64,
                    blocking_degree: degree,
                    blocking_letter: Some(letter.canonical_proportional_form()),
                    ..OrderVerifyResult::default()
                }));
            }
            if degree >= 2 && allow_algebraic_letters {
                let used = letter.used_variable_indices();
                if order[step + 1..]
                    .iter()
                    .any(|variable| used.contains(variable))
                {
                    return Ok(Some(OrderVerifyResult {
                        blocking_step: step as i64,
                        blocking_degree: degree,
                        forbidden_dep: true,
                        blocking_letter: Some(letter.canonical_proportional_form()),
                        ..OrderVerifyResult::default()
                    }));
                }
            }
        }
    }
    Ok(None)
}

fn build_set_table(
    groups: &[Vec<Poly>],
    variables: &[usize],
    engine: &mut ReductionEngine,
) -> Result<HashMap<u64, Vec<Vec<Poly>>>> {
    let mut table = HashMap::new();
    table.insert(0, groups.to_vec());
    for size in 1..=variables.len() {
        for bits in subsets_of_size(variables.len(), size) {
            engine.check_time("verify_order subset loop")?;
            let mut reduced_groups = vec![Vec::new(); groups.len()];
            for (group_index, output) in reduced_groups.iter_mut().enumerate() {
                let mut paths = Vec::with_capacity(size);
                for (bit, &pivot) in variables.iter().enumerate() {
                    if bits & (1_u64 << bit) == 0 {
                        continue;
                    }
                    let parent = bits ^ (1_u64 << bit);
                    let parent_groups = table.get(&parent).ok_or_else(|| {
                        Error::InvalidInput(
                            "verify_order_is_lr: missing parent subset state".into(),
                        )
                    })?;
                    paths.push(engine.reduce(&parent_groups[group_index], pivot, None)?);
                }
                *output = intersect_proportional(&paths);
            }
            table.insert(bits, reduced_groups);
        }
    }
    Ok(table)
}

/// Verify one specific integration order without searching for alternatives.
///
/// `order_var_indices` must be a permutation of `xvar_indices`.  A malformed
/// order is a normal negative result rather than an error, matching the JSON
/// protocol.  Carry-discharge is intentionally absent: this certifies the
/// strict/terminal algebraic-letter condition required by pinned execution.
pub fn verify_order_is_lr(
    group_polys: &[Vec<Poly>],
    xvar_indices: &[usize],
    order_var_indices: &[usize],
    allow_algebraic_letters: bool,
) -> Result<OrderVerifyResult> {
    let variable_count = xvar_indices.len();
    if order_var_indices.len() != variable_count {
        return Ok(OrderVerifyResult::malformed());
    }
    let variables = xvar_indices.iter().copied().collect::<HashSet<_>>();
    let order = order_var_indices.iter().copied().collect::<HashSet<_>>();
    if order.len() != variable_count || variables != order {
        return Ok(OrderVerifyResult::malformed());
    }
    if group_polys.is_empty() || variable_count == 0 {
        return Ok(OrderVerifyResult::accepted());
    }
    validate_contexts(group_polys, xvar_indices)?;

    let max_degree = configured_max_degree(allow_algebraic_letters)?;
    let mut engine = ReductionEngine::new()?;

    // Fast, sound accept: the single path over-approximates every refined
    // prefix.  A block is only provisional and must be adjudicated below.
    let mut current = group_polys.to_vec();
    let mut single_path_blocked = false;
    for (step, &pivot) in order_var_indices.iter().enumerate() {
        if first_blocker(
            &current,
            pivot,
            step,
            order_var_indices,
            max_degree,
            allow_algebraic_letters,
        )?
        .is_some()
        {
            single_path_blocked = true;
            break;
        }
        if step + 1 < variable_count {
            let mut next = Vec::with_capacity(current.len());
            for group in &current {
                next.push(engine.reduce(group, pivot, None)?);
            }
            current = next;
        }
    }
    if !single_path_blocked {
        return Ok(OrderVerifyResult::accepted());
    }

    if variable_count > 63 {
        return Err(Error::InvalidInput(
            "verify_order_is_lr: > 63 integration variables (bitmask overflow)".into(),
        ));
    }
    let table = build_set_table(group_polys, xvar_indices, &mut engine)?;
    let bit_of = xvar_indices
        .iter()
        .enumerate()
        .map(|(bit, &variable)| (variable, bit))
        .collect::<HashMap<_, _>>();

    let mut bits = 0_u64;
    for (step, &pivot) in order_var_indices.iter().enumerate() {
        let groups = table.get(&bits).ok_or_else(|| {
            Error::InvalidInput("verify_order_is_lr: missing prefix state".into())
        })?;
        if let Some(blocker) = first_blocker(
            groups,
            pivot,
            step,
            order_var_indices,
            max_degree,
            allow_algebraic_letters,
        )? {
            return Ok(blocker);
        }
        let bit = bit_of.get(&pivot).copied().ok_or_else(|| {
            Error::InvalidInput("verify_order_is_lr: order permutation changed".into())
        })?;
        bits |= 1_u64 << bit;
    }
    Ok(OrderVerifyResult::accepted())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::core::PolyCtx;

    use super::*;

    fn parse(ctx: &Arc<PolyCtx>, expression: &str) -> Poly {
        Poly::parse(ctx.clone(), expression).unwrap()
    }

    #[test]
    fn malformed_orders_are_negative_results_with_empty_diagnostics() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let groups = vec![vec![parse(&ctx, "x+y")]];
        for order in [&[0][..], &[0, 0], &[0, 2]] {
            let result = verify_order_is_lr(&groups, &[0, 1], order, false).unwrap();
            assert!(result.malformed);
            assert!(!result.is_lr);
            assert_eq!(result.blocking_step, -1);
            assert_eq!(result.blocking_degree, 0);
            assert!(!result.forbidden_dep);
            assert!(result.blocking_letter.is_none());
        }
    }

    #[test]
    fn reports_a_genuine_quadratic_obstruction() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let groups = vec![
            vec![parse(&ctx, "1+x+x^2"), parse(&ctx, "x"), parse(&ctx, "y")],
            vec![parse(&ctx, "1+y"), parse(&ctx, "x"), parse(&ctx, "y")],
        ];
        let result = verify_order_is_lr(&groups, &[0, 1], &[0, 1], false).unwrap();
        let repeated = verify_order_is_lr(&groups, &[0, 1], &[0, 1], false).unwrap();
        assert_eq!(repeated, result);
        assert!(!result.is_lr);
        assert!(!result.malformed);
        assert_eq!(result.blocking_step, 0);
        assert_eq!(result.blocking_degree, 2);
        assert!(!result.forbidden_dep);
        assert_eq!(
            result.blocking_letter,
            Some(parse(&ctx, "x^2+x+1").canonical_proportional_form())
        );
    }

    #[test]
    fn reports_a_forbidden_pending_variable_dependency() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let groups = vec![vec![parse(&ctx, "x^2*y+x+1")]];
        let result = verify_order_is_lr(&groups, &[0, 1], &[0, 1], true).unwrap();
        assert!(!result.is_lr);
        assert_eq!(result.blocking_step, 0);
        assert_eq!(result.blocking_degree, 2);
        assert!(result.forbidden_dep);
        assert_eq!(
            result.blocking_letter,
            Some(parse(&ctx, "x^2*y+x+1").canonical_proportional_form())
        );
    }

    #[test]
    fn accepts_empty_and_all_linear_inputs() {
        assert!(verify_order_is_lr(&[], &[], &[], false).unwrap().is_lr);

        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let groups = vec![
            vec![parse(&ctx, "x"), parse(&ctx, "1+x"), parse(&ctx, "y")],
            vec![parse(&ctx, "y"), parse(&ctx, "1+y"), parse(&ctx, "x")],
        ];
        assert!(
            verify_order_is_lr(&groups, &[0, 1], &[0, 1], false)
                .unwrap()
                .is_lr
        );
    }

    #[test]
    fn multigroup_intersection_drops_the_e26_spurious_quadratic() {
        let ctx = PolyCtx::new(["x2", "x4", "x5", "x6", "x7", "x8", "x9"]).unwrap();
        let group_a = [
            "1+x2",
            "1+x4",
            "x5*x6+x5*x7+x6*x7+x5*x6*x7+x5*x6*x8+x5*x7*x8+x6*x7*x8+x5*x9+x7*x9+x5*x7*x9+x5*x8*x9+x7*x8*x9",
            "x5*x6*x7+x5*x6*x8+x5*x7*x8+x6*x7*x8+x5*x6*x9+x6*x7*x9+x5*x6*x7*x9+x5*x8*x9+x5*x6*x8*x9+x7*x8*x9+x5*x7*x8*x9+x6*x7*x8*x9",
            "x2", "x4", "x5", "x6", "x7", "x8", "x9",
        ]
        .map(|expression| parse(&ctx, expression))
        .to_vec();
        let group_b = [
            "1+x2",
            "1+x4",
            "x5+x7+x5*x7+x5*x8+x7*x8",
            "1+x9",
            "x6+x9",
            "x2",
            "x4",
            "x5",
            "x6",
            "x7",
            "x8",
            "x9",
        ]
        .map(|expression| parse(&ctx, expression))
        .to_vec();

        let result = verify_order_is_lr(
            &[group_a, group_b],
            &[0, 1, 2, 3, 4, 5, 6],
            &[4, 2, 3, 6, 5, 1, 0],
            false,
        )
        .unwrap();
        assert!(result.is_lr, "{result:?}");
    }
}
