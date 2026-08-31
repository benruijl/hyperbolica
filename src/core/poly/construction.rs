use std::sync::Arc;

use symbolica::prelude::*;

use super::{Poly, PolyCtx, SymbolicaPoly};
use crate::error::{Error, Result};
use crate::symbols::SYMBOL_NAMESPACE;

impl Poly {
    pub fn zero(ctx: Arc<PolyCtx>) -> Self {
        let inner = SymbolicaPoly::new(&Q, None, ctx.variable_map());
        Self { ctx, inner }
    }

    pub fn one(ctx: Arc<PolyCtx>) -> Self {
        let inner = SymbolicaPoly::new(&Q, Some(1), ctx.variable_map()).one();
        Self { ctx, inner }
    }

    pub fn from_int(ctx: Arc<PolyCtx>, value: i64) -> Self {
        let template = SymbolicaPoly::new(&Q, Some(1), ctx.variable_map());
        let inner = template.constant(Q.nth(value.into()));
        Self { ctx, inner }
    }

    pub fn generator(ctx: Arc<PolyCtx>, variable: usize) -> Result<Self> {
        let symbol = ctx
            .variables
            .get(variable)
            .ok_or_else(|| Error::UnknownVariable(variable.to_string()))?
            .clone();
        let template = SymbolicaPoly::new(&Q, Some(1), ctx.variable_map());
        let inner = template.variable(&symbol).map_err(Error::InvalidInput)?;
        Ok(Self { ctx, inner })
    }

    pub fn parse(ctx: Arc<PolyCtx>, expression: &str) -> Result<Self> {
        let atom = Atom::parse(expression, SYMBOL_NAMESPACE, ParseSettings::default()).map_err(
            |message| Error::PolynomialParse {
                expression: expression.to_owned(),
                message,
            },
        )?;
        Self::from_atom(ctx, atom.as_view()).map_err(|error| Error::PolynomialParse {
            expression: expression.to_owned(),
            message: error.to_string(),
        })
    }

    /// Convert a Symbolica atom into the context's exact polynomial ring.
    pub fn from_atom(ctx: Arc<PolyCtx>, atom: AtomView<'_>) -> Result<Self> {
        let inner = atom
            .try_to_polynomial::<_, u16>(&Q, Some(ctx.variable_map()))
            .map_err(|error| Error::InvalidInput(error.to_string()))?;
        if inner.get_vars_ref() != ctx.variables.as_slice() {
            return Err(Error::InvalidInput(
                "atom contains a symbol outside the polynomial context".into(),
            ));
        }
        Ok(Self { ctx, inner })
    }

    /// Convert the exact polynomial back to a normalized Symbolica atom.
    pub fn to_atom(&self) -> Atom {
        self.inner.to_expression()
    }

    pub(crate) fn from_inner(ctx: Arc<PolyCtx>, inner: SymbolicaPoly) -> Self {
        Self { ctx, inner }
    }

    pub(crate) fn inner(&self) -> &SymbolicaPoly {
        &self.inner
    }

    pub fn ctx(&self) -> &Arc<PolyCtx> {
        &self.ctx
    }

    pub fn is_zero(&self) -> bool {
        self.inner.is_zero()
    }

    pub fn is_one(&self) -> bool {
        self.inner.is_one()
    }

    pub fn is_rational_constant(&self) -> bool {
        self.inner.is_constant()
    }

    /// Return the exact value when this polynomial is constant.
    pub fn rational_constant(&self) -> Option<Rational> {
        self.is_rational_constant()
            .then(|| self.inner.get_constant())
    }

    pub fn n_terms(&self) -> usize {
        self.inner.nterms()
    }

    pub fn total_degree(&self) -> u64 {
        self.inner
            .exponents_iter()
            .map(|exponents| exponents.iter().map(|&value| u64::from(value)).sum())
            .max()
            .unwrap_or(0)
    }

    pub fn equal(&self, other: &Self) -> bool {
        self.ctx.compatible_with(&other.ctx) && self.inner == other.inner
    }

    pub fn leading_coefficient_is_negative(&self) -> bool {
        !self.is_zero() && self.inner.lcoeff().numerator_ref().is_negative()
    }

    pub fn used_variable_indices(&self) -> Vec<usize> {
        (0..self.ctx.len())
            .filter(|&variable| self.inner.contains(variable))
            .collect()
    }

    pub fn transplant(
        &self,
        destination: Arc<PolyCtx>,
        source_to_destination: &[Option<usize>],
    ) -> Result<Self> {
        if source_to_destination.len() != self.ctx.len() {
            return Err(Error::InvalidInput(
                "transplant map length differs from source context".into(),
            ));
        }

        for &target in source_to_destination.iter().flatten() {
            if target >= destination.len() {
                return Err(Error::InvalidInput(format!(
                    "destination variable index {target} is out of range"
                )));
            }
        }

        // Symbolica's native rearrangement is the fastest exact path when the
        // map is induced by structural variable identity. It supports
        // permutations, context growth, and dropping unused variables.
        let identity_map = self
            .ctx
            .variables
            .iter()
            .map(|source| {
                destination
                    .variables
                    .iter()
                    .position(|target| target == source)
            })
            .collect::<Vec<_>>();
        if identity_map == source_to_destination {
            let inner = self
                .inner
                .rearrange_with_growth(destination.variables.as_slice())
                .map_err(Error::InvalidInput)?;
            return Ok(Self::from_inner(destination, inner));
        }

        // An explicit map may rename variables or identify several source
        // variables with one destination variable. Native `rearrange` does
        // not express those semantics, so retain the sparse term adapter and
        // merge powers (and any colliding monomials) exactly.
        let mut inner = SymbolicaPoly::new(&Q, Some(self.n_terms()), destination.variable_map());
        for (term, coefficient) in self
            .inner
            .exponents_iter()
            .zip(self.inner.coefficients.iter())
        {
            let mut exponents = vec![0_u16; destination.len()];
            for (source, &exponent) in term.iter().enumerate() {
                if exponent == 0 {
                    continue;
                }
                let target = source_to_destination[source].ok_or_else(|| {
                    Error::InvalidInput(format!(
                        "used source variable `{}` has no destination",
                        self.ctx.vars()[source]
                    ))
                })?;
                exponents[target] = exponents[target].checked_add(exponent).ok_or_else(|| {
                    Error::InvalidInput(format!(
                        "transplant exponent overflow at destination variable `{}`",
                        destination.vars()[target]
                    ))
                })?;
            }
            inner.append_monomial(coefficient.clone(), &exponents);
        }
        Ok(Self::from_inner(destination, inner))
    }
}
