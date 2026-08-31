use std::collections::HashSet;

use crate::core::Poly;
use crate::error::{Error, Result};

pub(super) fn subsets_of_size(n: usize, size: usize) -> Vec<u64> {
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

pub(super) fn validate_inputs(groups: &[Vec<Poly>], variables: &[usize]) -> Result<()> {
    if variables.len() > 63 {
        return Err(Error::InvalidInput(
            "find_lr_orders supports at most 63 integration variables".into(),
        ));
    }
    let mut unique = HashSet::new();
    if !variables.iter().all(|variable| unique.insert(*variable)) {
        return Err(Error::InvalidInput(
            "integration variable indices must be unique".into(),
        ));
    }
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

pub(super) fn configured_max_degree(algebraic: bool) -> Result<i64> {
    if !algebraic {
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
