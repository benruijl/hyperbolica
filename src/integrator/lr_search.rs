//! Linear-reducibility search by Brown/Fubini polynomial reduction.
//!
//! This is the Symbolica-backed counterpart of HyperFLINT's
//! `integrator/lr_search.cpp`. The important detail is the subset table:
//! the letters attached to a subset are the proportional intersection of
//! every one-variable path into that subset. A single reduction chain is
//! only an over-approximation and can produce false LR obstructions.

mod carry;
mod input;
mod search;

#[cfg(test)]
mod tests;

use crate::core::Poly;

pub(crate) use super::lr_reduction::ReductionEngine;
pub use super::lr_reduction::{
    SingCollector, dedup_proportional, intersect_proportional, leaf_count_proxy,
    lr_letter_admissible, st_fubini_lr,
};
pub use search::{find_lr_orders, find_lr_orders_collect};

/// Options for [`find_lr_orders`].
#[derive(Clone, Copy, Debug)]
pub struct LrSearchOptions {
    /// Admit quadratic letters in addition to linear letters.
    pub allow_algebraic_letters: bool,
    /// Carry pending square-root obligations to later pivots.
    pub carry_discharge: bool,
    /// Relative score cutoff at each subset size. `INFINITY` is exhaustive.
    pub score_prune_factor: f64,
    /// Apply the pure-Symbolica Euler-characteristic genuineness filter to
    /// each subset-table intersection.
    pub euler_filter: bool,
}

impl Default for LrSearchOptions {
    fn default() -> Self {
        Self {
            allow_algebraic_letters: false,
            carry_discharge: false,
            score_prune_factor: f64::INFINITY,
            euler_filter: false,
        }
    }
}

/// Best order and its HyperFLINT-compatible heuristic profile.
#[derive(Clone, Debug)]
pub struct LrResult {
    /// Original polynomial-context variable indices, in integration order.
    pub order: Vec<usize>,
    pub score: f64,
    /// Quadratic letters encountered along the selected order.
    pub root_polys: Vec<Poly>,
    pub carried_sqrts: u64,
    pub kin_sqrts: u64,
    pub terminal_quads: u64,
    /// Distinct obligations minted along a carry-discharge path.
    pub obligation_polys: Vec<Poly>,
}

impl LrResult {
    pub(super) fn empty(score: f64) -> Self {
        Self {
            order: Vec::new(),
            score,
            root_polys: Vec::new(),
            carried_sqrts: 0,
            kin_sqrts: 0,
            terminal_quads: 0,
            obligation_polys: Vec::new(),
        }
    }

    /// True when no linearly-reducible order exists.
    pub fn is_nolr(&self) -> bool {
        self.order.is_empty() && !self.score.is_finite()
    }
}
