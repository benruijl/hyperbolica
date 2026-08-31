use std::sync::Arc;

use super::{SymCoef, SymMonomial};
use crate::core::{PolyCtx, Rat};
use crate::error::{Error, Result};

impl SymCoef {
    pub fn zero(ctx: Arc<PolyCtx>) -> Self {
        Self {
            ctx,
            terms: Vec::new(),
        }
    }

    pub fn one(ctx: Arc<PolyCtx>) -> Self {
        Self::from_owned_rat(Rat::one(ctx))
    }

    pub fn from_rat(rational: &Rat) -> Self {
        Self::from_owned_rat(rational.clone())
    }

    fn from_owned_rat(rational: Rat) -> Self {
        let ctx = rational.ctx().clone();
        if rational.is_zero() {
            Self::zero(ctx)
        } else {
            Self {
                ctx,
                terms: vec![SymMonomial::new(rational)],
            }
        }
    }

    /// Build a value from raw monomials and put it in canonical form.
    ///
    /// This mirrors the infallible upstream constructor.  Use
    /// [`Self::try_from_monomials`] when the monomials may originate in an
    /// untrusted or independently constructed context.
    pub fn from_monomials(ctx: Arc<PolyCtx>, monomials: Vec<SymMonomial>) -> Self {
        Self::try_from_monomials(ctx, monomials)
            .expect("symbolic-coefficient monomial context mismatch")
    }

    pub fn try_from_monomials(ctx: Arc<PolyCtx>, monomials: Vec<SymMonomial>) -> Result<Self> {
        if monomials
            .iter()
            .any(|monomial| monomial.prefactor.ctx().vars() != ctx.vars())
        {
            return Err(Error::ContextMismatch);
        }
        Self::canonicalize_owned(ctx, monomials)
    }

    pub fn pi_factor(ctx: Arc<PolyCtx>) -> Self {
        let mut monomial = SymMonomial::new(Rat::one(ctx.clone()));
        monomial.pi_power = 1;
        Self {
            ctx,
            terms: vec![monomial],
        }
    }

    pub fn im_factor(ctx: Arc<PolyCtx>) -> Self {
        let mut monomial = SymMonomial::new(Rat::one(ctx.clone()));
        monomial.i_power = 1;
        Self {
            ctx,
            terms: vec![monomial],
        }
    }

    pub fn log_factor(ctx: Arc<PolyCtx>, argument: i64) -> Result<Self> {
        if argument <= 0 {
            return Err(Error::InvalidInput(format!(
                "logarithm argument must be positive, got {argument}"
            )));
        }
        let mut monomial = SymMonomial::new(Rat::one(ctx.clone()));
        monomial.log_powers.insert(argument, 1);
        Ok(Self {
            ctx,
            terms: vec![monomial],
        })
    }

    pub fn delta_factor(ctx: Arc<PolyCtx>, variable: impl Into<String>) -> Self {
        let mut monomial = SymMonomial::new(Rat::one(ctx.clone()));
        monomial.delta_powers.insert(variable.into(), 1);
        Self {
            ctx,
            terms: vec![monomial],
        }
    }

    pub fn period_factor(ctx: Arc<PolyCtx>, period: u32) -> Self {
        let mut monomial = SymMonomial::new(Rat::one(ctx.clone()));
        monomial.period_powers.insert(period, 1);
        Self {
            ctx,
            terms: vec![monomial],
        }
    }

    pub fn ctx(&self) -> &Arc<PolyCtx> {
        &self.ctx
    }

    pub fn terms(&self) -> &[SymMonomial] {
        &self.terms
    }

    pub fn is_zero(&self) -> bool {
        self.terms.is_empty()
    }

    pub fn is_one(&self) -> bool {
        self.terms.len() == 1 && self.terms[0].is_pure_rat() && self.terms[0].prefactor.is_one()
    }

    /// Whether this value can be represented as an ordinary rational
    /// function.  Zero is a rational value too.
    pub fn is_rat(&self) -> bool {
        self.terms.is_empty() || (self.terms.len() == 1 && self.terms[0].is_pure_rat())
    }

    pub fn as_rat(&self) -> Result<Rat> {
        match self.terms.as_slice() {
            [] => Ok(Rat::zero(self.ctx.clone())),
            [monomial] if monomial.is_pure_rat() => Ok(monomial.prefactor.clone()),
            _ => Err(Error::InvalidInput(
                "symbolic coefficient has residual symbolic powers".into(),
            )),
        }
    }

    /// Return an independently canonicalized copy.
    pub fn canonicalize(&self) -> Self {
        // Values constructed by this module are already canonical, but keeping
        // the operation public matches HyperFLINT and provides a cheap way to
        // reassert the invariant at API boundaries.
        Self::canonicalize_owned(self.ctx.clone(), self.terms.clone())
            .expect("valid symbolic coefficient became non-canonical")
    }

    pub(super) fn canonicalize_owned(
        ctx: Arc<PolyCtx>,
        mut monomials: Vec<SymMonomial>,
    ) -> Result<Self> {
        for monomial in &mut monomials {
            monomial.normalize();
        }
        monomials.retain(|monomial| !monomial.prefactor.is_zero());
        if monomials.len() <= 1 {
            return Ok(Self {
                ctx,
                terms: monomials,
            });
        }

        monomials.sort_unstable_by(SymMonomial::powers_cmp);
        let mut terms: Vec<SymMonomial> = Vec::with_capacity(monomials.len());
        for monomial in monomials {
            let merge = terms
                .last()
                .is_some_and(|previous| previous.same_powers(&monomial));
            if merge {
                let previous = terms.last_mut().expect("last term just checked");
                previous.prefactor = previous.prefactor.try_add(&monomial.prefactor)?;
                if previous.prefactor.is_zero() {
                    terms.pop();
                }
            } else {
                terms.push(monomial);
            }
        }
        Ok(Self { ctx, terms })
    }

    pub(super) fn require_same_context(&self, other: &Self) -> Result<()> {
        if self.ctx.vars() == other.ctx.vars() {
            Ok(())
        } else {
            Err(Error::ContextMismatch)
        }
    }
}
