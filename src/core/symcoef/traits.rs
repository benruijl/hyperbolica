use std::fmt::{Display, Formatter};
use std::ops::{Add, AddAssign, Mul, Neg, Sub};

use super::SymCoef;

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
        self.ctx.is_compatible_with(&other.ctx) && self.terms == other.terms
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
