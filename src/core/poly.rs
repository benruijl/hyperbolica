//! Exact Symbolica-backed polynomial façade.
//!
//! Public types remain defined in this module; their implementations are
//! grouped into private semantic child modules.

mod algebra;
mod construction;
mod context;
mod traits;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use symbolica::domains::rational::RationalField;
use symbolica::prelude::{MultivariatePolynomial, PolyVariable};

type SymbolicaPoly = MultivariatePolynomial<RationalField, u16>;

/// The ordered variable set shared by polynomials in one exact ring.
#[derive(Clone, Debug)]
pub struct PolyCtx {
    names: Arc<Vec<String>>,
    variables: Arc<Vec<PolyVariable>>,
}

/// An exact multivariate polynomial over the rationals.
///
/// Symbolica owns the sparse canonical representation. `Poly` only adds the
/// fixed variable context and the compatibility operations used by HyperFLINT.
#[derive(Clone, Debug)]
pub struct Poly {
    ctx: Arc<PolyCtx>,
    inner: SymbolicaPoly,
}

/// Factorization with a rational unit and irreducible polynomial bases.
#[derive(Clone, Debug)]
pub struct Factored {
    pub constant: String,
    pub factors: Vec<(Poly, usize)>,
}
