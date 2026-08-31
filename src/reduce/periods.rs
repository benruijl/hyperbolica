//! Period evaluation for the MZV alphabets used by HyperFLINT.

use std::sync::Arc;

use crate::core::{PolyCtx, Rat, SymCoef};
use crate::error::Result;
use crate::integrator::{RegKey, Regulator, RegulatorSym};
use crate::symbols::{Word, Wordlist};

use super::mzv_expansion::MzvExpansionTable;
use super::mzv_reduce::MzvReductionTable;

mod conversion;
mod evaluation;
mod fibration;

#[cfg(test)]
mod tests;

/// Convert a literal-integer wordlist to its MZV expression.
pub fn to_mzv(ctx: &Arc<PolyCtx>, wordlist: &Wordlist) -> Result<Rat> {
    conversion::to_mzv(ctx, wordlist)
}

pub fn to_mzv_with_expansion(
    ctx: &Arc<PolyCtx>,
    wordlist: &Wordlist,
    expansion: Option<&MzvExpansionTable>,
) -> Result<Rat> {
    conversion::to_mzv_with_expansion(ctx, wordlist, expansion)
}

/// Evaluate a period over `[0,1]` in the `{-1,0,1}` MZV alphabet.
pub fn zero_one_period(ctx: &Arc<PolyCtx>, word: &Word, table: &MzvReductionTable) -> Result<Rat> {
    evaluation::zero_one_period(ctx, word, table)
}

pub fn zero_one_period_with_expansion(
    ctx: &Arc<PolyCtx>,
    word: &Word,
    table: &MzvReductionTable,
    expansion: Option<&MzvExpansionTable>,
) -> Result<Rat> {
    evaluation::zero_one_period_with_expansion(ctx, word, table, expansion)
}

/// Evaluate a period over `[0,∞)` in HyperFLINT's `{-2,-1,0}` scope.
pub fn zero_inf_period(ctx: &Arc<PolyCtx>, word: &Word, table: &MzvReductionTable) -> Result<Rat> {
    evaluation::zero_inf_period(ctx, word, table)
}

pub fn zero_inf_period_with_expansion(
    ctx: &Arc<PolyCtx>,
    word: &Word,
    table: &MzvReductionTable,
    expansion: Option<&MzvExpansionTable>,
) -> Result<Rat> {
    evaluation::zero_inf_period_with_expansion(ctx, word, table, expansion)
}

/// Absorb every evaluable regulator key into the constant term.
pub fn evaluate_periods(
    ctx: &Arc<PolyCtx>,
    regulator: &Regulator,
    table: &MzvReductionTable,
) -> Result<Regulator> {
    evaluation::evaluate_periods(ctx, regulator, table)
}

/// A fibration-basis expansion with ordinary rational coefficients.
///
/// Each key slot corresponds to the variable at the same position in
/// [`Self::vars`]. An empty word is the multiplicative identity in that slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FibrationBasisResult {
    pub vars: Vec<String>,
    pub terms: Vec<(RegKey, Rat)>,
}

/// A fibration-basis expansion that can retain symbolic boundary residues.
///
/// If a terminal period cannot be evaluated by [`zero_inf_period`], its
/// regulator key is retained in the output in addition to the transformed
/// variable slots. This is the lossless counterpart to [`fibration_basis`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FibrationBasisResultSym {
    pub vars: Vec<String>,
    pub terms: Vec<(RegKey, SymCoef)>,
}

/// Project a rational regulator onto the ordered fibration basis.
///
/// Every requested variable is removed by [`crate::integrator::transform_shuffle`], then all
/// terminal words are evaluated as `[0,∞)` periods. The operation fails if a
/// terminal period is outside the supported alphabet or a transform produces
/// a genuinely symbolic boundary residue.
pub fn fibration_basis(
    ctx: &Arc<PolyCtx>,
    input: &Regulator,
    var_indices: &[usize],
    table: &MzvReductionTable,
) -> Result<FibrationBasisResult> {
    fibration::fibration_basis(ctx, input, var_indices, table)
}

/// Project a symbolic regulator onto the ordered fibration basis.
///
/// Unlike [`fibration_basis`], terminal periods outside the supported MZV
/// alphabet are retained as symbolic regulator-key factors instead of causing
/// the whole projection to fail.
pub fn fibration_basis_sym(
    ctx: &Arc<PolyCtx>,
    input: &RegulatorSym,
    var_indices: &[usize],
    table: &MzvReductionTable,
) -> Result<FibrationBasisResultSym> {
    fibration::fibration_basis_sym(ctx, input, var_indices, table)
}

/// Test exact vanishing after projection onto the complete fibration basis.
pub fn test_zero_function_sym(
    ctx: &Arc<PolyCtx>,
    regulator: &RegulatorSym,
    var_indices: &[usize],
    table: &MzvReductionTable,
) -> Result<bool> {
    fibration::test_zero_function_sym(ctx, regulator, var_indices, table)
}

/// HyperFLINT's Rat-valued phase-6e zero-function residue.
///
/// As in the upstream API, distinct unevaluated keys are assumed independent;
/// the returned Rat is zero when their accumulated residue vanishes.
pub fn test_zero_function(
    ctx: &Arc<PolyCtx>,
    regulator: &Regulator,
    table: &MzvReductionTable,
) -> Result<Rat> {
    evaluation::test_zero_function(ctx, regulator, table)
}
