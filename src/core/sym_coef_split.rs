//! Symbolic coefficients with narrow polynomial numerators and interned W-side factors.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use super::{
    FnIndexMaps, PolyCtx, Rat, SymCoef, SymMonomial, SymMonomialSplit, ZwTable,
    build_fn_index_maps, split_rat_by_w_monomial,
};
use crate::error::{Error, Result};

pub type SharedZwTable = Arc<Mutex<ZwTable>>;

/// A canonical symbolic sum whose rational prefactors remain factored as
/// `(narrow numerator, wide numerator handle, wide denominator handle)`.
#[derive(Clone, Debug)]
pub struct SymCoefSplit {
    wide_ctx: Arc<PolyCtx>,
    narrow_ctx: Arc<PolyCtx>,
    table: SharedZwTable,
    maps: Arc<FnIndexMaps>,
    terms: Vec<SymMonomialSplit>,
}

impl SymCoefSplit {
    pub fn new(
        wide_ctx: Arc<PolyCtx>,
        narrow_ctx: Arc<PolyCtx>,
        table: SharedZwTable,
    ) -> Result<Self> {
        {
            let guard = lock_table(&table)?;
            if guard.ctx().vars() != wide_ctx.vars() {
                return Err(Error::ContextMismatch);
            }
        }
        let maps = Arc::new(build_fn_index_maps(&wide_ctx, &narrow_ctx)?);
        Ok(Self {
            wide_ctx,
            narrow_ctx,
            table,
            maps,
            terms: Vec::new(),
        })
    }

    pub fn zero(
        wide_ctx: Arc<PolyCtx>,
        narrow_ctx: Arc<PolyCtx>,
        table: SharedZwTable,
    ) -> Result<Self> {
        Self::new(wide_ctx, narrow_ctx, table)
    }

    pub fn one(
        wide_ctx: Arc<PolyCtx>,
        narrow_ctx: Arc<PolyCtx>,
        table: SharedZwTable,
    ) -> Result<Self> {
        let one = SymCoef::one(wide_ctx);
        Self::from_symcoef(&one, narrow_ctx, table)
    }

    /// Lift a canonical wide-context [`SymCoef`] into the split representation.
    pub fn from_symcoef(
        source: &SymCoef,
        narrow_ctx: Arc<PolyCtx>,
        table: SharedZwTable,
    ) -> Result<Self> {
        let mut output = Self::new(source.ctx().clone(), narrow_ctx.clone(), table)?;
        {
            let mut zw = lock_table(&output.table)?;
            for monomial in source.terms() {
                let mut leaves = split_rat_by_w_monomial(
                    &monomial.prefactor,
                    narrow_ctx.clone(),
                    &mut zw,
                    &output.maps,
                )?;
                for leaf in &mut leaves {
                    leaf.pi_power = monomial.pi_power;
                    leaf.i_power = monomial.i_power;
                    leaf.log_powers = monomial.log_powers.clone();
                    leaf.delta_powers = monomial.delta_powers.clone();
                    leaf.period_powers = monomial.period_powers.clone();
                }
                output.terms.extend(leaves);
            }
        }
        output.canonicalize()
    }

    /// Upstream-compatible spelling: its argument is a `SymCoef`, whose
    /// monomial prefactors are the rational functions being split.
    pub fn from_rat(
        source: &SymCoef,
        narrow_ctx: Arc<PolyCtx>,
        table: SharedZwTable,
    ) -> Result<Self> {
        Self::from_symcoef(source, narrow_ctx, table)
    }

    pub fn from_terms(
        wide_ctx: Arc<PolyCtx>,
        narrow_ctx: Arc<PolyCtx>,
        table: SharedZwTable,
        terms: Vec<SymMonomialSplit>,
    ) -> Result<Self> {
        let mut result = Self::new(wide_ctx, narrow_ctx, table)?;
        if terms
            .iter()
            .any(|term| term.num_n.ctx().vars() != result.narrow_ctx.vars())
        {
            return Err(Error::ContextMismatch);
        }
        result.terms = terms;
        result.canonicalize()
    }

    pub fn wide_ctx(&self) -> &Arc<PolyCtx> {
        &self.wide_ctx
    }

    pub fn narrow_ctx(&self) -> &Arc<PolyCtx> {
        &self.narrow_ctx
    }

    pub fn table(&self) -> SharedZwTable {
        self.table.clone()
    }

    pub fn terms(&self) -> &[SymMonomialSplit] {
        &self.terms
    }

    pub fn is_zero(&self) -> bool {
        self.terms.is_empty()
    }

    pub fn is_one(&self) -> bool {
        self.terms.len() == 1
            && self.terms[0].num_n.is_one()
            && self.terms[0].num_zw == super::ZW_ONE
            && self.terms[0].den_zw == super::ZW_ONE
            && split_has_no_symbolic_powers(&self.terms[0])
    }

    /// Lower back to the ordinary wide rational prefactor representation.
    pub fn as_symcoef(&self) -> Result<SymCoef> {
        if self.is_zero() {
            return Ok(SymCoef::zero(self.wide_ctx.clone()));
        }
        let zw = lock_table(&self.table)?;
        let source_to_destination = self
            .maps
            .narrow_to_wide
            .iter()
            .copied()
            .map(Some)
            .collect::<Vec<_>>();
        let mut monomials = Vec::with_capacity(self.terms.len());
        for split in &self.terms {
            let narrow_in_wide = split
                .num_n
                .transplant(self.wide_ctx.clone(), &source_to_destination)?;
            let numerator = narrow_in_wide.try_mul(zw.get(split.num_zw)?)?;
            let prefactor = Rat::new(numerator, zw.get(split.den_zw)?.clone())?;
            let mut monomial = SymMonomial::new(prefactor);
            monomial.pi_power = split.pi_power;
            monomial.i_power = split.i_power;
            monomial.log_powers = split.log_powers.clone();
            monomial.delta_powers = split.delta_powers.clone();
            monomial.period_powers = split.period_powers.clone();
            monomials.push(monomial);
        }
        SymCoef::try_from_monomials(self.wide_ctx.clone(), monomials)
    }

    /// Canonicalize arbitrary leaves with one sort followed by a linear
    /// collection pass.  Already-canonical values are cheap to reassert.
    pub fn canonicalize(&self) -> Result<Self> {
        let terms = canonicalize_terms(self.terms.clone())?;
        Ok(Self {
            wide_ctx: self.wide_ctx.clone(),
            narrow_ctx: self.narrow_ctx.clone(),
            table: self.table.clone(),
            maps: self.maps.clone(),
            terms,
        })
    }

    /// Linear merge of two canonical split sums.
    pub fn try_add(&self, other: &Self) -> Result<Self> {
        self.require_compatible(other)?;
        let mut terms = Vec::with_capacity(self.terms.len() + other.terms.len());
        let mut left = 0;
        let mut right = 0;
        while left < self.terms.len() && right < other.terms.len() {
            let a = &self.terms[left];
            let b = &other.terms[right];
            match split_cmp(a, b) {
                Ordering::Less => {
                    terms.push(a.clone());
                    left += 1;
                }
                Ordering::Greater => {
                    terms.push(b.clone());
                    right += 1;
                }
                Ordering::Equal => {
                    let numerator = a.num_n.try_add(&b.num_n)?;
                    if !numerator.is_zero() {
                        let mut merged = a.clone();
                        merged.num_n = numerator;
                        terms.push(merged);
                    }
                    left += 1;
                    right += 1;
                }
            }
        }
        terms.extend(self.terms[left..].iter().cloned());
        terms.extend(other.terms[right..].iter().cloned());
        Ok(self.with_terms(terms))
    }

    pub fn try_sub(&self, other: &Self) -> Result<Self> {
        self.try_add(&other.negated())
    }

    pub fn negated(&self) -> Self {
        self.with_terms(
            self.terms
                .iter()
                .cloned()
                .map(|mut term| {
                    term.num_n = -&term.num_n;
                    term
                })
                .collect(),
        )
    }

    pub fn try_mul(&self, other: &Self) -> Result<Self> {
        self.require_compatible(other)?;
        if self.is_zero() || other.is_zero() {
            return Ok(self.with_terms(Vec::new()));
        }
        let capacity = self
            .terms
            .len()
            .checked_mul(other.terms.len())
            .ok_or_else(|| Error::InvalidInput("split symbolic product is too large".into()))?;
        let mut products = Vec::with_capacity(capacity);
        {
            // One lock for the complete cartesian product lets ZwTable's op
            // memoization work without lock/unlock traffic per handle pair.
            let mut zw = lock_table(&self.table)?;
            for left in &self.terms {
                for right in &other.terms {
                    let mut product = SymMonomialSplit::pure(
                        left.num_n.try_mul(&right.num_n)?,
                        zw.multiply(left.num_zw, right.num_zw)?,
                        zw.multiply(left.den_zw, right.den_zw)?,
                    );
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
        }
        let terms = canonicalize_terms(products)?;
        Ok(self.with_terms(terms))
    }

    pub fn try_mul_rat(&self, rational: &Rat) -> Result<Self> {
        if self.wide_ctx.vars() != rational.ctx().vars() {
            return Err(Error::ContextMismatch);
        }
        if self.is_zero() || rational.is_zero() {
            return Ok(self.with_terms(Vec::new()));
        }
        let mut products = Vec::new();
        {
            let mut zw = lock_table(&self.table)?;
            let leaves =
                split_rat_by_w_monomial(rational, self.narrow_ctx.clone(), &mut zw, &self.maps)?;
            products.reserve(self.terms.len().saturating_mul(leaves.len()));
            for term in &self.terms {
                for leaf in &leaves {
                    let mut product = term.clone();
                    product.num_n = product.num_n.try_mul(&leaf.num_n)?;
                    product.num_zw = zw.multiply(product.num_zw, leaf.num_zw)?;
                    product.den_zw = zw.multiply(product.den_zw, leaf.den_zw)?;
                    products.push(product);
                }
            }
        }
        let terms = canonicalize_terms(products)?;
        Ok(self.with_terms(terms))
    }

    pub fn try_div_rat(&self, rational: &Rat) -> Result<Self> {
        self.try_mul_rat(&rational.pow(-1)?)
    }

    pub fn equals_canonical(&self, other: &Self) -> bool {
        self.wide_ctx.vars() == other.wide_ctx.vars()
            && self.narrow_ctx.vars() == other.narrow_ctx.vars()
            && Arc::ptr_eq(&self.table, &other.table)
            && self.terms == other.terms
    }

    fn require_compatible(&self, other: &Self) -> Result<()> {
        if self.wide_ctx.vars() == other.wide_ctx.vars()
            && self.narrow_ctx.vars() == other.narrow_ctx.vars()
            && Arc::ptr_eq(&self.table, &other.table)
        {
            Ok(())
        } else {
            Err(Error::ContextMismatch)
        }
    }

    fn with_terms(&self, terms: Vec<SymMonomialSplit>) -> Self {
        Self {
            wide_ctx: self.wide_ctx.clone(),
            narrow_ctx: self.narrow_ctx.clone(),
            table: self.table.clone(),
            maps: self.maps.clone(),
            terms,
        }
    }
}

impl PartialEq for SymCoefSplit {
    fn eq(&self, other: &Self) -> bool {
        self.equals_canonical(other)
    }
}

impl Eq for SymCoefSplit {}

fn lock_table(table: &SharedZwTable) -> Result<MutexGuard<'_, ZwTable>> {
    table
        .lock()
        .map_err(|_| Error::InvalidInput("wide polynomial table is poisoned".into()))
}

fn split_cmp(left: &SymMonomialSplit, right: &SymMonomialSplit) -> Ordering {
    left.pi_power
        .cmp(&right.pi_power)
        .then_with(|| left.i_power.cmp(&right.i_power))
        .then_with(|| left.log_powers.cmp(&right.log_powers))
        .then_with(|| left.delta_powers.cmp(&right.delta_powers))
        .then_with(|| left.period_powers.cmp(&right.period_powers))
        .then_with(|| left.num_zw.cmp(&right.num_zw))
        .then_with(|| left.den_zw.cmp(&right.den_zw))
}

fn normalize_split(term: &mut SymMonomialSplit) {
    let i_residue = term.i_power.rem_euclid(4);
    if i_residue >= 2 {
        term.num_n = -&term.num_n;
    }
    term.i_power = i_residue % 2;
    term.log_powers.retain(|_, exponent| *exponent != 0);
    term.period_powers.retain(|_, exponent| *exponent != 0);
    term.delta_powers.retain(|_, exponent| {
        *exponent = exponent.rem_euclid(2);
        *exponent != 0
    });
}

fn canonicalize_terms(mut terms: Vec<SymMonomialSplit>) -> Result<Vec<SymMonomialSplit>> {
    for term in &mut terms {
        normalize_split(term);
    }
    terms.retain(|term| !term.num_n.is_zero());
    terms.sort_unstable_by(split_cmp);
    let mut canonical: Vec<SymMonomialSplit> = Vec::with_capacity(terms.len());
    for term in terms {
        if canonical
            .last()
            .is_some_and(|previous| split_cmp(previous, &term) == Ordering::Equal)
        {
            let previous = canonical.last_mut().expect("last term was checked");
            previous.num_n = previous.num_n.try_add(&term.num_n)?;
            if previous.num_n.is_zero() {
                canonical.pop();
            }
        } else {
            canonical.push(term);
        }
    }
    Ok(canonical)
}

fn add_powers<K: Ord + Clone>(target: &mut BTreeMap<K, i32>, source: &BTreeMap<K, i32>) {
    for (key, exponent) in source {
        *target.entry(key.clone()).or_insert(0) += exponent;
    }
}

fn split_has_no_symbolic_powers(term: &SymMonomialSplit) -> bool {
    term.pi_power == 0
        && term.i_power == 0
        && term.log_powers.is_empty()
        && term.delta_powers.is_empty()
        && term.period_powers.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contexts() -> (Arc<PolyCtx>, Arc<PolyCtx>, SharedZwTable) {
        let wide = PolyCtx::new(["x", "y", "s", "t"]).unwrap();
        let narrow = PolyCtx::new(["x", "y"]).unwrap();
        let table = Arc::new(Mutex::new(ZwTable::new(wide.clone())));
        (wide, narrow, table)
    }

    #[test]
    fn lifting_and_lowering_preserve_all_symbolic_powers() {
        let (wide, narrow, table) = contexts();
        let mut first = SymMonomial::new(Rat::parse(wide.clone(), "(s*x+t*y)/(s+1)").unwrap());
        first.pi_power = 2;
        first.i_power = 1;
        first.log_powers.insert(2, 3);
        first.delta_powers.insert("x".into(), 1);
        first.period_powers.insert(7, 2);
        let source = SymCoef::from_monomials(wide, vec![first]);
        let split = SymCoefSplit::from_symcoef(&source, narrow, table).unwrap();
        assert_eq!(split.terms().len(), 2);
        assert_eq!(split.as_symcoef().unwrap(), source);
    }

    #[test]
    fn addition_collects_narrow_numerators_without_wide_rat_arithmetic() {
        let (wide, narrow, table) = contexts();
        let mut a = SymMonomial::new(Rat::parse(wide.clone(), "s*x/(s+1)").unwrap());
        a.log_powers.insert(2, 1);
        let mut b = SymMonomial::new(Rat::parse(wide.clone(), "s*y/(s+1)").unwrap());
        b.log_powers.insert(2, 1);
        let sa = SymCoef::from_monomials(wide.clone(), vec![a]);
        let sb = SymCoef::from_monomials(wide, vec![b]);
        let a = SymCoefSplit::from_symcoef(&sa, narrow.clone(), table.clone()).unwrap();
        let b = SymCoefSplit::from_symcoef(&sb, narrow, table.clone()).unwrap();
        let before = table.lock().unwrap().stats();
        let sum = a.try_add(&b).unwrap();
        let after = table.lock().unwrap().stats();
        assert_eq!(after.multiply_calls, before.multiply_calls);
        assert_eq!(sum.as_symcoef().unwrap(), sa.try_add(&sb).unwrap());
        assert!(sum.try_sub(&sum).unwrap().is_zero());
    }

    #[test]
    fn multiplication_matches_wide_symcoef_and_normalizes_i_delta() {
        let (wide, narrow, table) = contexts();
        let mut ma = SymMonomial::new(Rat::parse(wide.clone(), "s*x/(t+1)").unwrap());
        ma.i_power = 1;
        ma.delta_powers.insert("x".into(), 1);
        ma.period_powers.insert(3, 1);
        let a = SymCoef::from_monomials(wide.clone(), vec![ma]);
        let split = SymCoefSplit::from_symcoef(&a, narrow, table).unwrap();
        let product = split.try_mul(&split).unwrap();
        assert_eq!(product.as_symcoef().unwrap(), a.try_mul(&a).unwrap());
        assert_eq!(product.terms()[0].i_power, 0);
        assert!(product.terms()[0].delta_powers.is_empty());
        assert_eq!(product.terms()[0].period_powers.get(&3), Some(&2));
    }
}
