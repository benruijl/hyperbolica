use std::collections::BTreeMap;
use std::sync::Arc;

use super::{PolyCtx, Rat};
use crate::error::Result;
use crate::reduce::MzvReductionTable;

mod arithmetic;
mod construction;
mod monomial;
mod reduction;
mod traits;

#[cfg(test)]
mod tests;

/// One term in a [`SymCoef`].
///
/// The rational prefactor lives in the ordinary polynomial context.  The
/// remaining fields form a small symbolic sidecar for constants that should
/// not increase the arity of every polynomial in the calculation.  Ordered
/// maps make both the canonical order and the printed representation
/// independent of insertion order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymMonomial {
    pub prefactor: Rat,
    pub pi_power: i32,
    pub i_power: i32,
    pub log_powers: BTreeMap<i64, i32>,
    /// Powers of formal delta generators keyed by native context index.
    pub delta_powers: BTreeMap<usize, i32>,
    pub period_powers: BTreeMap<u32, i32>,
}

/// A canonical sum of rational prefactors times symbolic monomials.
///
/// Every value produced through the public constructors satisfies these
/// invariants:
///
/// - terms are sorted by their symbolic powers;
/// - no two terms have the same symbolic powers;
/// - no term has a zero rational prefactor;
/// - `i_power` is zero or one, with powers of `I^2` folded into the sign;
/// - every delta power is one (even powers have been removed).
#[derive(Clone, Debug)]
pub struct SymCoef {
    ctx: Arc<PolyCtx>,
    terms: Vec<SymMonomial>,
}

/// Fold every even power of Pi into the MZV basis using
/// `Pi^(2k) = (6*mzv_2)^k`.
///
/// The reduction table is part of the upstream API because it defines the
/// active period basis.  The identity itself only needs the `mzv_2` variable;
/// when that variable is absent the symbolic coefficient is returned
/// unchanged and no information is discarded.
pub fn simplify_symcoef(coefficient: &SymCoef, table: &MzvReductionTable) -> Result<SymCoef> {
    reduction::simplify_symcoef(coefficient, table)
}

/// Simplify even Pi powers and require the result to be a plain rational
/// function with no residual symbolic generators.
pub fn reduce_to_rat(coefficient: &SymCoef, table: &MzvReductionTable) -> Result<Rat> {
    reduction::reduce_to_rat(coefficient, table)
}
