//! Construction, Atom conversion, and lazy compatibility views.

use std::sync::{Arc, OnceLock};

use symbolica::domains::rational_polynomial::FromNumeratorAndDenominator;
use symbolica::prelude::*;

use super::{CompatibilityViews, NativeRat, NativeStorage, Rat, RatInner};
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
        let native = NativeRat::new(&Z, ctx.variable_map());
        Self::from_native(ctx, native).expect("zero preserves the polynomial context")
    }

    pub fn one(ctx: Arc<PolyCtx>) -> Self {
        Self::from_int(ctx, 1)
    }

    pub fn from_int(ctx: Arc<PolyCtx>, value: i64) -> Self {
        let mut native = NativeRat::new(&Z, ctx.variable_map());
        native.numerator = native.numerator.constant(Integer::from(value));
        Self::from_native(ctx, native).expect("integer constants preserve the polynomial context")
    }

    /// Construct an exact scalar directly in the native integer coefficient
    /// ring, without allocating intermediate rational-coefficient polynomials.
    pub fn from_rational(ctx: Arc<PolyCtx>, value: Rational) -> Self {
        let template = NativeRat::new(&Z, ctx.variable_map());
        let native = NativeRat::from_num_den(
            template.numerator.constant(value.numerator_ref().clone()),
            template
                .denominator
                .constant(value.denominator_ref().clone()),
            &Z,
            true,
        );
        Self::from_native(ctx, native).expect("rational constants preserve the polynomial context")
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
        self.inner.native.to_expression()
    }

    pub fn numerator(&self) -> &Poly {
        &self.compatibility_views().numerator
    }

    pub fn denominator(&self) -> &Poly {
        &self.compatibility_views().denominator
    }

    pub fn ctx(&self) -> &Arc<PolyCtx> {
        &self.inner.ctx
    }

    pub fn is_zero(&self) -> bool {
        self.inner.native.is_zero()
    }

    pub fn is_one(&self) -> bool {
        self.inner.native.numerator.is_one() && self.inner.native.denominator.is_one()
    }

    /// Return the exact Symbolica scalar when this rational function is
    /// constant in every polynomial variable.
    ///
    /// This reads the canonical integer numerator and denominator directly;
    /// it does not materialize the compatibility [`Poly`] views or round-trip
    /// through expression formatting.
    pub fn rational_constant(&self) -> Option<Rational> {
        self.inner.native.is_constant().then(|| {
            Rational::from((
                self.inner.native.numerator.get_constant(),
                self.inner.native.denominator.get_constant(),
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
        self.same_context(other) && self.native() == other.native()
    }

    pub(crate) fn native(&self) -> &NativeRat {
        &self.inner.native
    }

    #[cfg(test)]
    pub(crate) fn compatibility_views_initialized(&self) -> bool {
        self.inner.views.get().is_some()
    }

    pub(crate) fn from_native(ctx: Arc<PolyCtx>, mut native: NativeRat) -> Result<Self> {
        if native.denominator.is_zero() {
            return Err(Error::DivisionByZero);
        }
        if !ctx.has_variable_map(native.get_variables())
            || !ctx.has_variable_map(native.denominator.variables())
        {
            let variables = ctx.native_variables();
            // Native coefficient-field operations may return a permutation
            // or a subset of the context. Match structural identities, not
            // diagnostic names; even an unused foreign declaration remains
            // an error at this boundary.
            if native
                .numerator
                .variables()
                .iter()
                .chain(native.denominator.variables().iter())
                .any(|variable| !variables.contains(variable))
            {
                return Err(Error::InvalidInput(
                    "rational function contains an indeterminate outside its context".into(),
                ));
            }
            let numerator = native
                .numerator
                .rearrange_with_growth(variables)
                .map_err(Error::InvalidInput)?;
            let denominator = native
                .denominator
                .rearrange_with_growth(variables)
                .map_err(Error::InvalidInput)?;
            // A permutation preserves coprimality but can change the leading
            // denominator sign. Restore native normalization without a GCD.
            native = NativeRat::from_num_den(numerator, denominator, &Z, false);
        }
        Ok(Self {
            inner: Arc::new(RatInner {
                ctx,
                native: NativeStorage::Owned(native),
                views: OnceLock::new(),
            }),
        })
    }

    fn compatibility_views(&self) -> &CompatibilityViews {
        self.inner.views.get_or_init(|| {
            let mut numerator = self
                .inner
                .native
                .numerator
                .clone()
                .map_coeff(|coefficient| Q.to_element_numerator(coefficient.clone()), Q);
            let mut denominator = self
                .inner
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

            Box::new(CompatibilityViews {
                numerator: Poly::from_inner(self.inner.ctx.clone(), numerator),
                denominator: Poly::from_inner(self.inner.ctx.clone(), denominator),
            })
        })
    }

    pub(super) fn same_context(&self, other: &Self) -> bool {
        self.inner.ctx.is_compatible_with(&other.inner.ctx)
    }

    /// Share an unchanged value while preserving the caller's diagnostic
    /// context. Arithmetic only calls this after checking compatibility.
    pub(super) fn clone_in_context(&self, ctx: &Arc<PolyCtx>) -> Self {
        if Arc::ptr_eq(&self.inner.ctx, ctx) {
            self.clone()
        } else {
            Self {
                inner: Arc::new(RatInner {
                    ctx: ctx.clone(),
                    native: NativeStorage::Shared(match &self.inner.native {
                        NativeStorage::Owned(_) => self.inner.clone(),
                        NativeStorage::Shared(root) => root.clone(),
                    }),
                    views: OnceLock::new(),
                }),
            }
        }
    }

    pub(super) fn require_same_context(&self, other: &Self) -> Result<()> {
        if self.same_context(other) {
            Ok(())
        } else {
            Err(Error::ContextMismatch)
        }
    }
}
