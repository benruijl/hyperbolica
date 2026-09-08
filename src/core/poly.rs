//! Exact Symbolica-backed polynomial façade.
//!
//! Public types remain defined in this module; their implementations are
//! grouped into private semantic child modules.

mod algebra;
mod construction;
mod context;
mod integer;
mod traits;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use symbolica::domains::rational::RationalField;
use symbolica::prelude::{MultivariatePolynomial, PolyVariable, Rational};

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

/// Public Symbolica resultant kernels available for production selection and
/// differential benchmarking.
///
/// [`ResultantStrategy::Auto`] is the production default. It delegates the
/// representation-specific choice between optimized integer-associate Ducos
/// and modular CRT to Symbolica's [`symbolica::poly::PolynomialResultant`]
/// implementation for `Q`. The direct recurrence over rational
/// coefficients remains available only as a diagnostic baseline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResultantStrategy {
    #[default]
    Auto,
    /// Clear rational scalar denominators/content once, then run Ducos over Z.
    Ducos,
    /// Run the Ducos recurrence directly over rational polynomial coefficients.
    RationalDucos,
    Brown,
    Primitive,
    Crt,
}

/// Factorization with a rational unit and irreducible polynomial bases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Factored {
    pub constant: Rational,
    pub factors: Vec<(Poly, usize)>,
}
