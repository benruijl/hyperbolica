//! Exact scalar separation for arithmetic over the integer coefficient ring.

use symbolica::prelude::{Field, IntegerRing, MultivariatePolynomial, Q, Rational, RingOps, Z};

use super::{Poly, SymbolicaPoly};

impl Poly {
    /// Return an integer polynomial and exact scalar whose product is `self`.
    /// The integer polynomial need not be primitive. Integer inputs use scale
    /// one without computing coefficient content; rational inputs remove that
    /// content once instead of reducing fractions inside every polynomial op.
    pub(crate) fn integer_associate(&self) -> (MultivariatePolynomial<IntegerRing, u16>, Rational) {
        integer_associate(&self.inner)
    }
}

pub(super) fn integer_associate(
    polynomial: &SymbolicaPoly,
) -> (MultivariatePolynomial<IntegerRing, u16>, Rational) {
    if polynomial.coefficients.iter().all(Rational::is_integer) {
        return (
            polynomial.map_coeff(|coefficient| coefficient.numerator_ref().clone(), Z),
            Rational::one(),
        );
    }
    let scalar = polynomial.content();
    let integer = polynomial.map_coeff(|coefficient| Q.div(coefficient, &scalar).numerator(), Z);
    (integer, scalar)
}

pub(super) fn restore_scalar(
    polynomial: &MultivariatePolynomial<IntegerRing, u16>,
    scalar: &Rational,
) -> SymbolicaPoly {
    if scalar.is_one() {
        polynomial.map_coeff(|coefficient| Rational::from(coefficient), Q)
    } else {
        polynomial.map_coeff(|coefficient| Q.mul(&Rational::from(coefficient), scalar), Q)
    }
}
