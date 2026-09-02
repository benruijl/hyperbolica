//! Canonical rational functions backed by Symbolica.
//!
//! The public type and its lazily materialized compatibility views live in
//! this facade. Semantic implementation blocks are split into child modules
//! so the native integer rational polynomial remains the single source of
//! truth across construction, arithmetic, and typed operations.

mod arithmetic;
mod representation;
mod traits;
mod typed_ops;

#[cfg(test)]
mod property_tests;
#[cfg(test)]
mod tests;

use std::sync::{Arc, OnceLock};

use symbolica::prelude::{IntegerRing, RationalPolynomial};

use super::{Poly, PolyCtx};

pub(crate) type NativeRat = RationalPolynomial<IntegerRing, u16>;

#[derive(Clone, Debug)]
struct CompatibilityViews {
    numerator: Poly,
    denominator: Poly,
}

/// A canonical rational function over the polynomial context.
///
/// Symbolica's integer-coefficient [`RationalPolynomial`] is the source of
/// truth. The Q-polynomial numerator and denominator are compatibility views
/// for the surrounding HyperFLINT-shaped algorithms and are materialized only
/// when requested. Clones share both the native value and the lazy views.
#[derive(Clone, Debug)]
pub struct Rat {
    ctx: Arc<PolyCtx>,
    native: Arc<NativeRat>,
    views: Arc<OnceLock<CompatibilityViews>>,
}
