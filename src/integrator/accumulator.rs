//! Balanced, bounded-storage accumulation of canonical coefficients.

use crate::core::{Rat, SymCoef};
use crate::error::Result;

pub(super) trait AddCoefficient: Sized {
    fn add_coefficient(&self, other: &Self) -> Result<Self>;
}

impl AddCoefficient for Rat {
    fn add_coefficient(&self, other: &Self) -> Result<Self> {
        self.try_add(other)
    }
}

impl AddCoefficient for SymCoef {
    fn add_coefficient(&self, other: &Self) -> Result<Self> {
        Self::merge_sorted_canonical(self, other)
    }
}

/// Each occupied level holds the sum of `2^level` consecutive inputs.
/// Equal-sized partial sums merge as binary carries, avoiding repeated
/// additions to one ever-growing rational expression. Only logarithmically
/// many partial sums are retained, and all of them remain canonical.
pub(super) struct BalancedSum<T> {
    levels: Vec<Option<T>>,
}

impl<T: AddCoefficient> BalancedSum<T> {
    pub(super) fn new(first: T) -> Self {
        Self {
            levels: vec![Some(first)],
        }
    }

    pub(super) fn push(&mut self, mut value: T) -> Result<()> {
        for slot in &mut self.levels {
            if let Some(previous) = slot.take() {
                value = previous.add_coefficient(&value)?;
            } else {
                *slot = Some(value);
                return Ok(());
            }
        }
        self.levels.push(Some(value));
        Ok(())
    }

    pub(super) fn finish(self) -> Result<T> {
        let mut values = self.levels.into_iter().flatten();
        // new() supplies the first coefficient, and a successful push never
        // decreases the represented input count to zero.
        let first = values.next().expect("nonempty coefficient accumulator");
        values.try_fold(first, |sum, value| value.add_coefficient(&sum))
    }

    #[cfg(test)]
    pub(super) fn materialize(&self) -> Result<T>
    where
        T: Clone,
    {
        Self {
            levels: self.levels.clone(),
        }
        .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{PolyCtx, SymMonomial};

    #[test]
    fn rational_sum_matches_left_fold_without_polynomial_compatibility_views() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let inputs = [
            "1/(x+1)",
            "y/(x+1)^2",
            "-1/(x+1)",
            "1/((x+1)*(y+1))",
            "-y/(x+1)^2",
            "x/(y+1)",
            "-1/((x+1)*(y+1))",
            "1/(x+y)",
            "-x/(y+1)",
        ]
        .into_iter()
        .map(|expression| Rat::parse(ctx.clone(), expression).unwrap())
        .collect::<Vec<_>>();
        for len in 1..=inputs.len() {
            let mut sum = BalancedSum::new(inputs[0].clone());
            let mut expected = inputs[0].clone();
            for value in &inputs[1..len] {
                sum.push(value.clone()).unwrap();
                expected = expected.try_add(value).unwrap();
            }
            assert_eq!(sum.finish().unwrap(), expected);
        }
        assert!(
            inputs
                .iter()
                .all(|value| !value.compatibility_views_initialized())
        );
    }

    #[test]
    fn symbolic_sum_preserves_distinct_powers_and_uses_logarithmic_storage() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let mut sum = BalancedSum::new(SymCoef::zero(ctx.clone()));
        let mut expected = SymCoef::zero(ctx.clone());
        for index in 0..257 {
            let mut monomial = SymMonomial::new(Rat::from_int(ctx.clone(), index - 128));
            monomial.pi_power = (index % 3) as i32;
            monomial.period_powers.insert(7, (index % 5) as i32);
            let value = SymCoef::from_monomials(ctx.clone(), vec![monomial]);
            expected = expected.try_add(&value).unwrap();
            sum.push(value).unwrap();
            assert!(sum.levels.len() <= 9);
        }
        assert_eq!(sum.finish().unwrap(), expected);
    }
}
