//! Projective Cheng--Wu gauge scan and HyperFLINT's strict/FindRoots
//! keep rules.

mod projective;
mod search;

#[cfg(test)]
mod tests;

pub use super::lr_find_roots::{
    FrJudgment, PathState, conic_rationalizable, fr_judge, step_fr_judge,
};
pub use projective::projective_input;
pub use search::find_lr_orders_scan;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeepRule {
    Strict,
    FindRoots,
}

/// Exponent `a + b epsilon` of one input factor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScanExponent {
    pub a: i64,
    pub b: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct ScanOptions {
    pub keep_rule: KeepRule,
    /// Apply the pure-Symbolica Euler-characteristic genuineness filter.
    pub euler_filter: bool,
    pub max_orders: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            keep_rule: KeepRule::Strict,
            euler_filter: false,
            max_orders: 8192,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScanOrder {
    pub order: Vec<usize>,
    pub gauge: usize,
    pub score: f64,
    pub carried_sqrts: u64,
    pub kin_sqrts: u64,
    pub terminal_quads: u64,
}

#[derive(Clone, Debug, Default)]
pub struct ScanResult {
    pub projective: bool,
    pub truncated: bool,
    pub orders: Vec<ScanOrder>,
}
