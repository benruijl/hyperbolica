//! Operator traits for factored rational functions.

use std::ops::{Add, Div, Mul, Neg, Sub};

use super::FactoredRat;

impl Add<&FactoredRat> for &FactoredRat {
    type Output = FactoredRat;

    fn add(self, rhs: &FactoredRat) -> Self::Output {
        self.try_add(rhs)
            .expect("factored-rational addition failed")
    }
}

impl Sub<&FactoredRat> for &FactoredRat {
    type Output = FactoredRat;

    fn sub(self, rhs: &FactoredRat) -> Self::Output {
        self.try_sub(rhs)
            .expect("factored-rational subtraction failed")
    }
}

impl Mul<&FactoredRat> for &FactoredRat {
    type Output = FactoredRat;

    fn mul(self, rhs: &FactoredRat) -> Self::Output {
        self.try_mul(rhs)
            .expect("factored-rational multiplication failed")
    }
}

impl Div<&FactoredRat> for &FactoredRat {
    type Output = FactoredRat;

    fn div(self, rhs: &FactoredRat) -> Self::Output {
        self.try_div(rhs)
            .expect("factored-rational division failed")
    }
}

impl Neg for &FactoredRat {
    type Output = FactoredRat;

    fn neg(self) -> Self::Output {
        self.negated()
    }
}
