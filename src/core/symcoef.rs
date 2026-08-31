use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter, Write};
use std::ops::{Add, AddAssign, Mul, Neg, Sub};
use std::sync::Arc;

use super::{Poly, PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::reduce::MzvReductionTable;

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
    pub delta_powers: BTreeMap<String, i32>,
    pub period_powers: BTreeMap<u32, i32>,
}

impl SymMonomial {
    pub fn new(prefactor: Rat) -> Self {
        Self {
            prefactor,
            pi_power: 0,
            i_power: 0,
            log_powers: BTreeMap::new(),
            delta_powers: BTreeMap::new(),
            period_powers: BTreeMap::new(),
        }
    }

    /// A stable representation of the symbolic powers, excluding the
    /// rational prefactor.
    pub fn power_key(&self) -> String {
        let mut key = format!("P{}|I{}|L", self.pi_power, self.i_power);
        for (argument, exponent) in &self.log_powers {
            let _ = write!(key, "{argument}:{exponent},");
        }
        key.push_str("|D");
        for (name, exponent) in &self.delta_powers {
            let _ = write!(key, "{name}:{exponent},");
        }
        key.push_str("|Q");
        for (period, exponent) in &self.period_powers {
            let _ = write!(key, "{period}:{exponent},");
        }
        key
    }

    pub fn is_pure_rat(&self) -> bool {
        self.pi_power == 0
            && self.i_power == 0
            && self.log_powers.is_empty()
            && self.delta_powers.is_empty()
            && self.period_powers.is_empty()
    }

    fn powers_cmp(&self, other: &Self) -> Ordering {
        self.pi_power
            .cmp(&other.pi_power)
            .then_with(|| self.i_power.cmp(&other.i_power))
            .then_with(|| self.log_powers.cmp(&other.log_powers))
            .then_with(|| self.delta_powers.cmp(&other.delta_powers))
            .then_with(|| self.period_powers.cmp(&other.period_powers))
    }

    fn same_powers(&self, other: &Self) -> bool {
        self.powers_cmp(other) == Ordering::Equal
    }

    fn normalize(&mut self) {
        // Reduce modulo four first so negative powers have the expected
        // algebraic meaning too: I^-1 = -I and I^-2 = -1.
        let residue = self.i_power.rem_euclid(4);
        if residue >= 2 {
            self.prefactor = self.prefactor.negated();
        }
        self.i_power = residue % 2;

        self.log_powers.retain(|_, exponent| *exponent != 0);
        self.period_powers.retain(|_, exponent| *exponent != 0);
        self.delta_powers.retain(|_, exponent| {
            *exponent = exponent.rem_euclid(2);
            *exponent != 0
        });
    }
}

impl Display for SymMonomial {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "({})", self.prefactor)?;

        match self.pi_power {
            0 => {}
            1 => formatter.write_str("*Pi")?,
            exponent => write!(formatter, "*Pi^{exponent}")?,
        }
        match self.i_power {
            0 => {}
            1 => formatter.write_str("*I")?,
            exponent => write!(formatter, "*I^{exponent}")?,
        }
        for (argument, exponent) in &self.log_powers {
            write!(formatter, "*Log[{argument}]")?;
            if *exponent != 1 {
                write!(formatter, "^{exponent}")?;
            }
        }
        for (name, exponent) in &self.delta_powers {
            write!(formatter, "*delta[{name}]")?;
            if *exponent != 1 {
                write!(formatter, "^{exponent}")?;
            }
        }
        for (period, exponent) in &self.period_powers {
            write!(formatter, "*Period[{period}]")?;
            if *exponent != 1 {
                write!(formatter, "^{exponent}")?;
            }
        }
        Ok(())
    }
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

    fn canonicalize_owned(ctx: Arc<PolyCtx>, mut monomials: Vec<SymMonomial>) -> Result<Self> {
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

    fn require_same_context(&self, other: &Self) -> Result<()> {
        if self.ctx.vars() == other.ctx.vars() {
            Ok(())
        } else {
            Err(Error::ContextMismatch)
        }
    }

    /// Linear merge of two already-canonical symbolic sums.
    pub fn merge_sorted_canonical(left: &Self, right: &Self) -> Result<Self> {
        left.merge_with(right, false)
    }

    fn merge_with(&self, other: &Self, subtract_other: bool) -> Result<Self> {
        self.require_same_context(other)?;
        let mut terms = Vec::with_capacity(self.terms.len() + other.terms.len());
        let mut left = 0;
        let mut right = 0;

        while left < self.terms.len() && right < other.terms.len() {
            let a = &self.terms[left];
            let b = &other.terms[right];
            match a.powers_cmp(b) {
                Ordering::Less => {
                    terms.push(a.clone());
                    left += 1;
                }
                Ordering::Greater => {
                    let mut term = b.clone();
                    if subtract_other {
                        term.prefactor = term.prefactor.negated();
                    }
                    terms.push(term);
                    right += 1;
                }
                Ordering::Equal => {
                    let prefactor = if subtract_other {
                        a.prefactor.try_sub(&b.prefactor)?
                    } else {
                        a.prefactor.try_add(&b.prefactor)?
                    };
                    if !prefactor.is_zero() {
                        let mut term = a.clone();
                        term.prefactor = prefactor;
                        terms.push(term);
                    }
                    left += 1;
                    right += 1;
                }
            }
        }
        terms.extend(self.terms[left..].iter().cloned());
        if subtract_other {
            terms.extend(other.terms[right..].iter().cloned().map(|mut term| {
                term.prefactor = term.prefactor.negated();
                term
            }));
        } else {
            terms.extend(other.terms[right..].iter().cloned());
        }
        Ok(Self {
            ctx: self.ctx.clone(),
            terms,
        })
    }

    pub fn try_add(&self, other: &Self) -> Result<Self> {
        Self::merge_sorted_canonical(self, other)
    }

    pub fn try_sub(&self, other: &Self) -> Result<Self> {
        self.merge_with(other, true)
    }

    pub fn negated(&self) -> Self {
        Self {
            ctx: self.ctx.clone(),
            terms: self
                .terms
                .iter()
                .cloned()
                .map(|mut term| {
                    term.prefactor = term.prefactor.negated();
                    term
                })
                .collect(),
        }
    }

    /// Cartesian product of monomials followed by canonical collection.
    pub fn try_mul(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        if self.is_zero() || other.is_zero() {
            return Ok(Self::zero(self.ctx.clone()));
        }

        let capacity = self
            .terms
            .len()
            .checked_mul(other.terms.len())
            .ok_or_else(|| Error::InvalidInput("symbolic product is too large".into()))?;
        let mut products = Vec::with_capacity(capacity);
        for left in &self.terms {
            for right in &other.terms {
                let mut product = SymMonomial::new(left.prefactor.try_mul(&right.prefactor)?);
                product.pi_power = left.pi_power + right.pi_power;
                product.i_power = left.i_power + right.i_power;
                product.log_powers = left.log_powers.clone();
                add_powers(&mut product.log_powers, &right.log_powers);
                product.delta_powers = left.delta_powers.clone();
                add_powers(&mut product.delta_powers, &right.delta_powers);
                product.period_powers = left.period_powers.clone();
                add_powers(&mut product.period_powers, &right.period_powers);
                products.push(product);
            }
        }
        Self::canonicalize_owned(self.ctx.clone(), products)
    }

    pub fn try_mul_rat(&self, rational: &Rat) -> Result<Self> {
        if self.ctx.vars() != rational.ctx().vars() {
            return Err(Error::ContextMismatch);
        }
        if self.is_zero() || rational.is_zero() {
            return Ok(Self::zero(self.ctx.clone()));
        }
        let mut terms = Vec::with_capacity(self.terms.len());
        for mut term in self.terms.iter().cloned() {
            term.prefactor = term.prefactor.try_mul(rational)?;
            terms.push(term);
        }
        Ok(Self {
            ctx: self.ctx.clone(),
            terms,
        })
    }

    pub fn try_div_rat(&self, rational: &Rat) -> Result<Self> {
        if self.ctx.vars() != rational.ctx().vars() {
            return Err(Error::ContextMismatch);
        }
        if rational.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let mut terms = Vec::with_capacity(self.terms.len());
        for mut term in self.terms.iter().cloned() {
            term.prefactor = term.prefactor.try_div(rational)?;
            terms.push(term);
        }
        Ok(Self {
            ctx: self.ctx.clone(),
            terms,
        })
    }
}

/// Fold every even power of Pi into the MZV basis using
/// `Pi^(2k) = (6*mzv_2)^k`.
///
/// The reduction table is part of the upstream API because it defines the
/// active period basis.  The identity itself only needs the `mzv_2` variable;
/// when that variable is absent the symbolic coefficient is returned
/// unchanged and no information is discarded.
pub fn simplify_symcoef(coefficient: &SymCoef, _table: &MzvReductionTable) -> Result<SymCoef> {
    let mzv_two = crate::symbols::mzv_atom(&[2]);
    let Some(mzv_two_index) = coefficient.ctx().index_of_indeterminate(mzv_two.as_view()) else {
        return Ok(coefficient.clone());
    };
    let six_mzv2 = Rat::from_poly(Poly::generator(coefficient.ctx().clone(), mzv_two_index)?)
        .try_mul(&Rat::from_int(coefficient.ctx().clone(), 6))?;
    let mut monomials = Vec::with_capacity(coefficient.terms().len());
    for mut monomial in coefficient.terms().iter().cloned() {
        // Euclidean division also gives the mathematically correct reduction
        // for negative powers: Pi^-2 = (6*mzv_2)^-1.
        let pairs = monomial.pi_power.div_euclid(2);
        monomial.pi_power = monomial.pi_power.rem_euclid(2);
        if pairs != 0 {
            monomial.prefactor = monomial
                .prefactor
                .try_mul(&six_mzv2.pow(i64::from(pairs))?)?;
        }
        monomials.push(monomial);
    }
    SymCoef::try_from_monomials(coefficient.ctx().clone(), monomials)
}

/// Simplify even Pi powers and require the result to be a plain rational
/// function with no residual symbolic generators.
pub fn reduce_to_rat(coefficient: &SymCoef, table: &MzvReductionTable) -> Result<Rat> {
    let simplified = simplify_symcoef(coefficient, table)?;
    for monomial in simplified.terms() {
        let residual = if monomial.pi_power != 0 {
            Some(format!("residual Pi^{}", monomial.pi_power))
        } else if monomial.i_power != 0 {
            Some("residual imaginary unit I".into())
        } else if !monomial.log_powers.is_empty() {
            Some("residual Log factor".into())
        } else if !monomial.delta_powers.is_empty() {
            Some("residual delta factor".into())
        } else if !monomial.period_powers.is_empty() {
            Some("residual period generator".into())
        } else {
            None
        };
        if let Some(residual) = residual {
            return Err(Error::InvalidInput(format!("reduce_to_rat: {residual}")));
        }
    }
    simplified.as_rat()
}

fn add_powers<K: Ord + Clone>(target: &mut BTreeMap<K, i32>, source: &BTreeMap<K, i32>) {
    for (key, exponent) in source {
        *target.entry(key.clone()).or_insert(0) += exponent;
    }
}

impl Display for SymCoef {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        if self.terms.is_empty() {
            return formatter.write_str("0");
        }
        for (index, monomial) in self.terms.iter().enumerate() {
            if index != 0 {
                formatter.write_str(" + ")?;
            }
            Display::fmt(monomial, formatter)?;
        }
        Ok(())
    }
}

impl PartialEq for SymCoef {
    fn eq(&self, other: &Self) -> bool {
        self.ctx.vars() == other.ctx.vars() && self.terms == other.terms
    }
}

impl Eq for SymCoef {}

impl Add<&SymCoef> for &SymCoef {
    type Output = SymCoef;

    fn add(self, rhs: &SymCoef) -> Self::Output {
        self.try_add(rhs)
            .expect("symbolic-coefficient addition failed")
    }
}

impl Sub<&SymCoef> for &SymCoef {
    type Output = SymCoef;

    fn sub(self, rhs: &SymCoef) -> Self::Output {
        self.try_sub(rhs)
            .expect("symbolic-coefficient subtraction failed")
    }
}

impl Mul<&SymCoef> for &SymCoef {
    type Output = SymCoef;

    fn mul(self, rhs: &SymCoef) -> Self::Output {
        self.try_mul(rhs)
            .expect("symbolic-coefficient multiplication failed")
    }
}

impl Neg for &SymCoef {
    type Output = SymCoef;

    fn neg(self) -> Self::Output {
        self.negated()
    }
}

impl AddAssign<&SymCoef> for SymCoef {
    fn add_assign(&mut self, rhs: &SymCoef) {
        *self = self
            .try_add(rhs)
            .expect("symbolic-coefficient addition failed");
    }
}

#[cfg(test)]
mod tests {
    use symbolica::prelude::Symbol;

    use crate::symbols::{SYMBOL_NAMESPACE, legacy, mzv_atom};

    use super::*;

    fn context() -> Arc<PolyCtx> {
        let x = Symbol::parse("x", SYMBOL_NAMESPACE).unwrap();
        PolyCtx::from_indeterminates([x.to_atom(), mzv_atom(&[2]), mzv_atom(&[3])]).unwrap()
    }

    fn rat(ctx: &Arc<PolyCtx>, expression: &str) -> Rat {
        let atom = legacy::parse_expression(expression).unwrap();
        Rat::from_atom(ctx.clone(), atom.as_view()).unwrap()
    }

    #[test]
    fn canonicalizes_i_and_delta_powers() {
        let ctx = context();
        let mut inverse_i = SymMonomial::new(Rat::from_int(ctx.clone(), 3));
        inverse_i.i_power = -1;
        inverse_i.delta_powers.insert("x".into(), -3);
        inverse_i.log_powers.insert(2, 0);

        let canonical = SymCoef::from_monomials(ctx.clone(), vec![inverse_i]);
        assert_eq!(canonical.terms()[0].i_power, 1);
        assert_eq!(canonical.terms()[0].delta_powers["x"], 1);
        assert!(canonical.terms()[0].log_powers.is_empty());
        assert_eq!(
            canonical.terms()[0].prefactor,
            Rat::from_int(ctx.clone(), -3)
        );

        let i = SymCoef::im_factor(ctx.clone());
        assert_eq!((&i * &i).as_rat().unwrap(), Rat::from_int(ctx.clone(), -1));
        assert_eq!(&(&i * &i) * &(&i * &i), SymCoef::one(ctx.clone()));

        let delta = SymCoef::delta_factor(ctx.clone(), "x");
        assert_eq!((&delta * &delta).as_rat().unwrap(), Rat::one(ctx));
    }

    #[test]
    fn canonical_collection_and_merge_add_sub() {
        let ctx = context();
        let mut first = SymMonomial::new(rat(&ctx, "mzv_3/x"));
        first.pi_power = 1;
        let mut second = SymMonomial::new(rat(&ctx, "6*mzv_2"));
        second.pi_power = 1;
        let combined = SymCoef::from_monomials(ctx.clone(), vec![second, first]);
        assert_eq!(combined.terms().len(), 1);
        assert_eq!(combined.terms()[0].prefactor, rat(&ctx, "mzv_3/x+6*mzv_2"));

        let pi = SymCoef::pi_factor(ctx.clone());
        let log = SymCoef::log_factor(ctx.clone(), 2).unwrap();
        let left = &combined + &log;
        let right = &pi + &log;
        let difference = left.try_sub(&right).unwrap();
        assert_eq!(difference.terms().len(), 1);
        assert_eq!(difference.terms()[0].pi_power, 1);
        assert_eq!(
            difference.terms()[0].prefactor,
            rat(&ctx, "mzv_3/x+6*mzv_2-1")
        );
    }

    #[test]
    fn multiplication_is_a_cartesian_product() {
        let ctx = context();
        let pi = SymCoef::pi_factor(ctx.clone());
        let log = SymCoef::log_factor(ctx.clone(), 2).unwrap();
        let imaginary = SymCoef::im_factor(ctx.clone());
        let delta = SymCoef::delta_factor(ctx.clone(), "x");

        let product = &(&pi + &log) * &(&imaginary + &delta);
        assert_eq!(product.terms().len(), 4);
        assert!(
            product
                .terms()
                .iter()
                .any(|term| term.pi_power == 1 && term.i_power == 1)
        );
        assert!(
            product
                .terms()
                .iter()
                .any(|term| term.log_powers.get(&2) == Some(&1)
                    && term.delta_powers.get("x") == Some(&1))
        );
    }

    #[test]
    fn formatting_is_deterministic_and_matches_hyperflint() {
        let ctx = context();
        let mut symbolic = SymMonomial::new(rat(&ctx, "mzv_3/(x+1)"));
        symbolic.pi_power = 2;
        symbolic.i_power = 5;
        symbolic.log_powers.insert(7, 2);
        symbolic.log_powers.insert(2, 1);
        symbolic.delta_powers.insert("z".into(), 1);
        symbolic.delta_powers.insert("x".into(), 1);
        symbolic.period_powers.insert(12, 2);
        symbolic.period_powers.insert(3, 1);

        let coefficient = SymCoef::from_monomials(ctx, vec![symbolic]);
        let formatted = coefficient.to_string();
        assert!(formatted.contains("MZV(3)"));
        assert!(formatted.contains("*Pi^2*I*Log[2]*Log[7]^2"));
        assert_eq!(
            coefficient.terms()[0].power_key(),
            "P2|I1|L2:1,7:2,|Dx:1,z:1,|Q3:1,12:2,"
        );
    }

    #[test]
    fn rational_scaling_and_unwrap() {
        let ctx = context();
        let mzv = rat(&ctx, "mzv_3/(1+x)");
        let pure = SymCoef::from_rat(&mzv);
        assert!(pure.is_rat());
        assert_eq!(pure.as_rat().unwrap(), mzv);

        let scaled = SymCoef::pi_factor(ctx.clone())
            .try_mul_rat(&rat(&ctx, "6*mzv_2"))
            .unwrap();
        assert!(scaled.to_string().contains("MZV(2)"));
        assert!(!scaled.is_rat());
        assert!(scaled.as_rat().is_err());
        assert!(SymCoef::zero(ctx.clone()).is_rat());
        assert_eq!(SymCoef::zero(ctx.clone()).as_rat().unwrap(), Rat::zero(ctx));
    }

    #[test]
    fn even_pi_powers_fold_into_mzv_two() {
        let ctx = context();
        let mut monomial = SymMonomial::new(rat(&ctx, "2*x"));
        monomial.pi_power = 4;
        let coefficient = SymCoef::from_monomials(ctx.clone(), vec![monomial]);
        let table = MzvReductionTable::default();
        let simplified = simplify_symcoef(&coefficient, &table).unwrap();
        assert_eq!(simplified.terms()[0].pi_power, 0);
        assert_eq!(simplified.as_rat().unwrap(), rat(&ctx, "72*x*mzv_2^2"));
        assert_eq!(
            reduce_to_rat(&coefficient, &table).unwrap(),
            rat(&ctx, "72*x*mzv_2^2")
        );
    }

    #[test]
    fn reduction_refuses_residual_symbolic_generators() {
        let ctx = context();
        let odd_pi = SymCoef::pi_factor(ctx.clone());
        let logarithm = SymCoef::log_factor(ctx, 2).unwrap();
        let table = MzvReductionTable::default();
        assert!(reduce_to_rat(&odd_pi, &table).is_err());
        assert!(reduce_to_rat(&logarithm, &table).is_err());
    }
}
