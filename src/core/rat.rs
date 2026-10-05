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
mod context_tests;
#[cfg(test)]
mod power_tests;
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
    // A clone touches one value-local reference count, not the context shared
    // by every coefficient in a parallel integration.
    inner: Arc<RatInner>,
}

#[derive(Debug)]
struct RatInner {
    ctx: Arc<PolyCtx>,
    native: NativeStorage,
    views: OnceLock<Box<CompatibilityViews>>,
}

#[derive(Debug)]
enum NativeStorage {
    // Ordinary values need one allocation for the complete shared handle data.
    Owned(NativeRat),
    // Identity arithmetic can preserve a different caller's diagnostic context
    // without copying polynomial buffers. This always points to an Owned root;
    // repeated context changes cannot create chains of retained wrappers.
    Shared(Arc<RatInner>),
}

impl AsRef<NativeRat> for NativeStorage {
    #[inline]
    fn as_ref(&self) -> &NativeRat {
        match self {
            Self::Owned(value) => value,
            Self::Shared(root) => root.native.as_ref(),
        }
    }
}

impl std::ops::Deref for NativeStorage {
    type Target = NativeRat;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}
