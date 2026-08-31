use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::{Add, Mul, Neg, Sub};

use super::Poly;

impl Display for Poly {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        if self.inner.is_zero() {
            return formatter.write_str("0");
        }

        for term_index in (0..self.inner.nterms()).rev() {
            let coefficient = self.inner.coefficients[term_index].to_string();
            let (negative, magnitude) = coefficient
                .strip_prefix('-')
                .map_or((false, coefficient.as_str()), |magnitude| (true, magnitude));
            if term_index == self.inner.nterms() - 1 {
                if negative {
                    formatter.write_str("-")?;
                }
            } else if negative {
                formatter.write_str(" - ")?;
            } else {
                formatter.write_str(" + ")?;
            }

            let exponents = self.inner.exponents(term_index);
            let has_monomial = exponents.iter().any(|&exponent| exponent != 0);
            if !has_monomial || magnitude != "1" {
                formatter.write_str(magnitude)?;
                if has_monomial {
                    formatter.write_str("*")?;
                }
            }

            let mut first_variable = true;
            for (name, &exponent) in self.ctx.vars().iter().zip(exponents) {
                if exponent == 0 {
                    continue;
                }
                if !first_variable {
                    formatter.write_str("*")?;
                }
                first_variable = false;
                formatter.write_str(name)?;
                if exponent != 1 {
                    write!(formatter, "^{exponent}")?;
                }
            }
        }
        Ok(())
    }
}

impl PartialEq for Poly {
    fn eq(&self, other: &Self) -> bool {
        self.equal(other)
    }
}

impl Eq for Poly {}

impl Hash for Poly {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // `Poly::eq` deliberately includes the complete Hyperbolica context,
        // not only Symbolica's polynomial payload. In particular, native
        // constant-polynomial equality ignores the variable map, while two
        // `Poly` constants in incompatible contexts are not equal. Prefix the
        // exact context components used by `PolyCtx::compatible_with` before
        // delegating the canonical coefficient/exponent representation to
        // Symbolica so `Hash` and `Eq` have identical semantics.
        self.ctx.names.hash(state);
        self.ctx.variables.hash(state);
        self.inner.hash(state);
    }
}

impl Add<&Poly> for &Poly {
    type Output = Poly;

    fn add(self, rhs: &Poly) -> Self::Output {
        self.try_add(rhs).expect("polynomial context mismatch")
    }
}

impl Sub<&Poly> for &Poly {
    type Output = Poly;

    fn sub(self, rhs: &Poly) -> Self::Output {
        self.try_sub(rhs).expect("polynomial context mismatch")
    }
}

impl Mul<&Poly> for &Poly {
    type Output = Poly;

    fn mul(self, rhs: &Poly) -> Self::Output {
        self.try_mul(rhs).expect("polynomial context mismatch")
    }
}

impl Neg for &Poly {
    type Output = Poly;

    fn neg(self) -> Self::Output {
        Poly::from_inner(self.ctx.clone(), -self.inner.clone())
    }
}
