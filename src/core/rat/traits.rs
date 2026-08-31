//! Formatting, equality, and operator traits.

use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::{Add, Div, Mul, Neg, Sub};

use super::Rat;

fn has_top_level(expression: &str, needle: char) -> bool {
    let mut depth = 0_i32;
    for character in expression.chars().skip(1) {
        match character {
            '(' => depth += 1,
            ')' => depth -= 1,
            character if depth == 0 && character == needle => return true,
            _ => {}
        }
    }
    false
}

fn wrap_numerator(expression: String) -> String {
    if ['+', '-', ' ']
        .into_iter()
        .any(|needle| has_top_level(&expression, needle))
    {
        format!("({expression})")
    } else {
        expression
    }
}

fn wrap_denominator(expression: String) -> String {
    if ['+', '-', '*', ' ']
        .into_iter()
        .any(|needle| has_top_level(&expression, needle))
    {
        format!("({expression})")
    } else {
        expression
    }
}

impl Display for Rat {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        if self.denominator().is_one() {
            Display::fmt(self.numerator(), formatter)
        } else {
            write!(
                formatter,
                "{}/{}",
                wrap_numerator(self.numerator().to_string()),
                wrap_denominator(self.denominator().to_string())
            )
        }
    }
}

impl PartialEq for Rat {
    fn eq(&self, other: &Self) -> bool {
        self.equal(other)
    }
}

impl Eq for Rat {}

impl Hash for Rat {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Native `RationalPolynomial` equality and hashing are structural,
        // but constant polynomials intentionally ignore their variable maps.
        // `Rat::Eq` is stricter: it also requires the complete Hyperbolica
        // context (diagnostic names and structural PolyVariables). Prefix the
        // exact fields used by `same_context` so Hash and Eq stay consistent.
        self.ctx.vars().hash(state);
        self.ctx.variable_map().hash(state);
        self.native.as_ref().hash(state);
    }
}

impl Add<&Rat> for &Rat {
    type Output = Rat;

    fn add(self, rhs: &Rat) -> Self::Output {
        self.try_add(rhs)
            .expect("rational-function addition failed")
    }
}

impl Sub<&Rat> for &Rat {
    type Output = Rat;

    fn sub(self, rhs: &Rat) -> Self::Output {
        self.try_sub(rhs)
            .expect("rational-function subtraction failed")
    }
}

impl Mul<&Rat> for &Rat {
    type Output = Rat;

    fn mul(self, rhs: &Rat) -> Self::Output {
        self.try_mul(rhs)
            .expect("rational-function multiplication failed")
    }
}

impl Div<&Rat> for &Rat {
    type Output = Rat;

    fn div(self, rhs: &Rat) -> Self::Output {
        self.try_div(rhs)
            .expect("rational-function division failed")
    }
}

impl Neg for &Rat {
    type Output = Rat;

    fn neg(self) -> Self::Output {
        self.negated()
    }
}
