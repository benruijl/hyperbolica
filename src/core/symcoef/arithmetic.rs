use std::cmp::Ordering;
use std::collections::BTreeMap;

use super::{SymCoef, SymMonomial};
use crate::core::Rat;
use crate::error::{Error, Result};

impl SymCoef {
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

fn add_powers<K: Ord + Clone>(target: &mut BTreeMap<K, i32>, source: &BTreeMap<K, i32>) {
    for (key, exponent) in source {
        *target.entry(key.clone()).or_insert(0) += exponent;
    }
}
