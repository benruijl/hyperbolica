//! Construction, Atom conversion, and lazy compatibility views.

use std::sync::{Arc, OnceLock};

use symbolica::domains::rational_polynomial::FromNumeratorAndDenominator;
use symbolica::prelude::*;

use super::{CompatibilityViews, NativeRat, Rat};
use crate::core::{Poly, PolyCtx};
use crate::error::{Error, Result};
use crate::symbols::SYMBOL_NAMESPACE;

impl Rat {
    pub fn new(numerator: Poly, denominator: Poly) -> Result<Self> {
        if !numerator.ctx().is_compatible_with(denominator.ctx()) {
            return Err(Error::ContextMismatch);
        }
        if denominator.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let ctx = numerator.ctx().clone();
        let native = NativeRat::from_num_den(
            numerator.inner().clone(),
            denominator.inner().clone(),
            &Z,
            true,
        );
        Self::from_native(ctx, native)
    }

    pub fn from_poly(polynomial: Poly) -> Self {
        let ctx = polynomial.ctx().clone();
        let denominator = polynomial.inner().one();
        let native = NativeRat::from_num_den(polynomial.inner().clone(), denominator, &Z, false);
        Self::from_native(ctx, native).expect("polynomial conversion produced an invalid rational")
    }

    pub fn zero(ctx: Arc<PolyCtx>) -> Self {
        Self::from_poly(Poly::zero(ctx))
    }

    pub fn one(ctx: Arc<PolyCtx>) -> Self {
        Self::from_poly(Poly::one(ctx))
    }

    pub fn from_int(ctx: Arc<PolyCtx>, value: i64) -> Self {
        Self::from_poly(Poly::from_int(ctx, value))
    }

    pub fn parse(ctx: Arc<PolyCtx>, expression: &str) -> Result<Self> {
        let atom = Atom::parse(expression, SYMBOL_NAMESPACE, ParseSettings::default()).map_err(
            |message| Error::RationalParse {
                expression: expression.to_owned(),
                message,
            },
        )?;
        Self::from_atom(ctx, atom.as_view()).map_err(|error| Error::RationalParse {
            expression: expression.to_owned(),
            message: error.to_string(),
        })
    }

    /// Convert a Symbolica atom into a canonical exact rational function.
    pub fn from_atom(ctx: Arc<PolyCtx>, atom: AtomView<'_>) -> Result<Self> {
        let native = atom
            .try_to_rational_polynomial::<_, _, u16>(&Q, &Z, Some(ctx.variable_map()))
            .map_err(|error| Error::InvalidInput(error.to_string()))?;
        Self::from_native(ctx, native)
    }

    /// Convert the exact rational function back to a normalized Symbolica atom.
    pub fn to_atom(&self) -> Atom {
        self.native.to_expression()
    }

    pub fn numerator(&self) -> &Poly {
        &self.compatibility_views().numerator
    }

    pub fn denominator(&self) -> &Poly {
        &self.compatibility_views().denominator
    }

    pub fn ctx(&self) -> &Arc<PolyCtx> {
        &self.ctx
    }

    pub fn is_zero(&self) -> bool {
        self.native.is_zero()
    }

    pub fn is_one(&self) -> bool {
        self.native.numerator.is_one() && self.native.denominator.is_one()
    }

    /// Return the exact Symbolica scalar when this rational function is
    /// constant in every polynomial variable.
    ///
    /// This reads the canonical integer numerator and denominator directly;
    /// it does not materialize the compatibility [`Poly`] views or round-trip
    /// through expression formatting.
    pub fn rational_constant(&self) -> Option<Rational> {
        self.native.is_constant().then(|| {
            Rational::from((
                self.native.numerator.get_constant(),
                self.native.denominator.get_constant(),
            ))
        })
    }

    /// Return the exact Symbolica integer when this rational function is an
    /// integer constant.
    pub fn integer_constant(&self) -> Option<Integer> {
        let value = self.rational_constant()?;
        value.is_integer().then(|| value.numerator_ref().clone())
    }

    pub fn equal(&self, other: &Self) -> bool {
        self.same_context(other) && self.native == other.native
    }

    pub(crate) fn native(&self) -> &NativeRat {
        &self.native
    }

    pub(crate) fn from_native(ctx: Arc<PolyCtx>, native: NativeRat) -> Result<Self> {
        if native.denominator.is_zero() {
            return Err(Error::DivisionByZero);
        }
        if native.get_variables().as_ref() != ctx.variable_map().as_ref()
            || native.denominator.get_vars_ref() != ctx.variable_map().as_ref()
        {
            return Err(Error::InvalidInput(
                "rational function contains an indeterminate outside its context".into(),
            ));
        }
        Ok(Self {
            ctx,
            native: Arc::new(native),
            views: Arc::new(OnceLock::new()),
        })
    }

    fn compatibility_views(&self) -> &CompatibilityViews {
        self.views.get_or_init(|| {
            let mut numerator = self
                .native
                .numerator
                .clone()
                .map_coeff(|coefficient| Q.to_element_numerator(coefficient.clone()), Q);
            let mut denominator = self
                .native
                .denominator
                .clone()
                .map_coeff(|coefficient| Q.to_element_numerator(coefficient.clone()), Q);

            // Keep the established public Poly views monic. This is only a
            // scalar presentation normalization; all CAS operations use the
            // canonical native integer rational polynomial above.
            let leading = denominator.lcoeff();
            if !Q.is_one(&leading) {
                numerator = numerator.div_coeff(&leading);
                denominator = denominator.div_coeff(&leading);
            }

            CompatibilityViews {
                numerator: Poly::from_inner(self.ctx.clone(), numerator),
                denominator: Poly::from_inner(self.ctx.clone(), denominator),
            }
        })
    }

    pub(super) fn same_context(&self, other: &Self) -> bool {
        self.ctx.is_compatible_with(&other.ctx)
    }

    pub(super) fn require_same_context(&self, other: &Self) -> Result<()> {
        if self.same_context(other) {
            Ok(())
        } else {
            Err(Error::ContextMismatch)
        }
    }
}
