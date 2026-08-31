use std::fmt::{Display, Formatter};
use std::ops::{Add, Div, Mul, Neg, Sub};
use std::sync::{Arc, OnceLock};

use symbolica::domains::rational::RationalField;
use symbolica::domains::rational_polynomial::FromNumeratorAndDenominator;
use symbolica::prelude::*;

use super::{Poly, PolyCtx};
use crate::error::{Error, Result};
use crate::symbols::SYMBOL_NAMESPACE;

pub(crate) type NativeRat = RationalPolynomial<IntegerRing, u16>;
type RationalPoly = MultivariatePolynomial<RationalField, u16>;

fn lift_native_polynomial(polynomial: &MultivariatePolynomial<IntegerRing, u16>) -> RationalPoly {
    polynomial.map_coeff(|coefficient| Q.to_element_numerator(coefficient.clone()), Q)
}

#[derive(Clone, Debug)]
struct CompatibilityViews {
    numerator: Poly,
    denominator: Poly,
}

/// A canonical rational function over the polynomial context.
///
/// Symbolica's integer-coefficient [`RationalPolynomial`] is the source of
/// truth. The Q-polynomial numerator and denominator are compatibility views
/// for the surrounding HyperFLINT-shaped algorithms and are materialized only
/// when requested. Clones share both the native value and the lazy views.
#[derive(Clone, Debug)]
pub struct Rat {
    ctx: Arc<PolyCtx>,
    native: Arc<NativeRat>,
    views: Arc<OnceLock<CompatibilityViews>>,
}

impl Rat {
    pub fn new(numerator: Poly, denominator: Poly) -> Result<Self> {
        if numerator.ctx().vars() != denominator.ctx().vars()
            || numerator.ctx().variable_map() != denominator.ctx().variable_map()
        {
            return Err(Error::ContextMismatch);
        }
        if denominator.is_zero() {
            return Err(Error::DivisionByZero);
        }

        let ctx = numerator.ctx().clone();
        let native = NativeRat::from_num_den(
            numerator.inner().clone(),
            denominator.inner().clone(),
            &Z,
            true,
        );
        Self::from_native(ctx, native)
    }

    pub fn from_poly(polynomial: Poly) -> Self {
        let ctx = polynomial.ctx().clone();
        let denominator = polynomial.inner().one();
        let native = NativeRat::from_num_den(polynomial.inner().clone(), denominator, &Z, false);
        Self::from_native(ctx, native).expect("polynomial conversion produced an invalid rational")
    }

    pub fn zero(ctx: Arc<PolyCtx>) -> Self {
        Self::from_poly(Poly::zero(ctx))
    }

    pub fn one(ctx: Arc<PolyCtx>) -> Self {
        Self::from_poly(Poly::one(ctx))
    }

    pub fn from_int(ctx: Arc<PolyCtx>, value: i64) -> Self {
        Self::from_poly(Poly::from_int(ctx, value))
    }

    pub fn parse(ctx: Arc<PolyCtx>, expression: &str) -> Result<Self> {
        let atom = Atom::parse(expression, SYMBOL_NAMESPACE, ParseSettings::default()).map_err(
            |message| Error::RationalParse {
                expression: expression.to_owned(),
                message,
            },
        )?;
        Self::from_atom(ctx, atom.as_view()).map_err(|error| Error::RationalParse {
            expression: expression.to_owned(),
            message: error.to_string(),
        })
    }

    /// Convert a Symbolica atom into a canonical exact rational function.
    pub fn from_atom(ctx: Arc<PolyCtx>, atom: AtomView<'_>) -> Result<Self> {
        let native = atom
            .try_to_rational_polynomial::<_, _, u16>(&Q, &Z, Some(ctx.variable_map()))
            .map_err(|error| Error::InvalidInput(error.to_string()))?;
        Self::from_native(ctx, native)
    }

    /// Convert the exact rational function back to a normalized Symbolica atom.
    pub fn to_atom(&self) -> Atom {
        self.native.to_expression()
    }

    pub fn numerator(&self) -> &Poly {
        &self.compatibility_views().numerator
    }

    pub fn denominator(&self) -> &Poly {
        &self.compatibility_views().denominator
    }

    pub fn ctx(&self) -> &Arc<PolyCtx> {
        &self.ctx
    }

    pub fn is_zero(&self) -> bool {
        self.native.is_zero()
    }

    pub fn is_one(&self) -> bool {
        self.native.numerator.is_one() && self.native.denominator.is_one()
    }

    pub fn equal(&self, other: &Self) -> bool {
        self.same_context(other) && self.native == other.native
    }

    pub(crate) fn native(&self) -> &NativeRat {
        &self.native
    }

    pub(crate) fn from_native(ctx: Arc<PolyCtx>, native: NativeRat) -> Result<Self> {
        if native.denominator.is_zero() {
            return Err(Error::DivisionByZero);
        }
        if native.get_variables().as_ref() != ctx.variable_map().as_ref()
            || native.denominator.get_vars_ref() != ctx.variable_map().as_ref()
        {
            return Err(Error::InvalidInput(
                "rational function contains an indeterminate outside its context".into(),
            ));
        }
        Ok(Self {
            ctx,
            native: Arc::new(native),
            views: Arc::new(OnceLock::new()),
        })
    }

    fn compatibility_views(&self) -> &CompatibilityViews {
        self.views.get_or_init(|| {
            let mut numerator = self
                .native
                .numerator
                .clone()
                .map_coeff(|coefficient| Q.to_element_numerator(coefficient.clone()), Q);
            let mut denominator = self
                .native
                .denominator
                .clone()
                .map_coeff(|coefficient| Q.to_element_numerator(coefficient.clone()), Q);

            // Keep the established public Poly views monic. This is only a
            // scalar presentation normalization; all CAS operations use the
            // canonical native integer rational polynomial above.
            let leading = denominator.lcoeff();
            if !Q.is_one(&leading) {
                numerator = numerator.div_coeff(&leading);
                denominator = denominator.div_coeff(&leading);
            }

            CompatibilityViews {
                numerator: Poly::from_inner(self.ctx.clone(), numerator),
                denominator: Poly::from_inner(self.ctx.clone(), denominator),
            }
        })
    }

    fn same_context(&self, other: &Self) -> bool {
        self.ctx.vars() == other.ctx.vars() && self.ctx.variable_map() == other.ctx.variable_map()
    }

    fn require_same_context(&self, other: &Self) -> Result<()> {
        if self.same_context(other) {
            Ok(())
        } else {
            Err(Error::ContextMismatch)
        }
    }

    pub fn try_add(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        Self::from_native(
            self.ctx.clone(),
            self.native.as_ref() + other.native.as_ref(),
        )
    }

    pub fn try_sub(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        Self::from_native(
            self.ctx.clone(),
            self.native.as_ref() - other.native.as_ref(),
        )
    }

    pub fn try_mul(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        Self::from_native(
            self.ctx.clone(),
            self.native.as_ref() * other.native.as_ref(),
        )
    }

    pub fn try_div(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        if other.is_zero() {
            return Err(Error::DivisionByZero);
        }
        Self::from_native(
            self.ctx.clone(),
            self.native.as_ref() / other.native.as_ref(),
        )
    }

    pub fn negated(&self) -> Self {
        Self::from_native(self.ctx.clone(), self.native.as_ref().clone().neg())
            .expect("negation preserves a rational function's context")
    }

    pub fn pow(&self, exponent: i64) -> Result<Self> {
        let magnitude = exponent
            .checked_abs()
            .ok_or(Error::InvalidExponent(exponent))? as u64;
        if magnitude > u64::from(u32::MAX) {
            return Err(Error::InvalidExponent(exponent));
        }
        let native = if exponent >= 0 {
            self.native.pow(magnitude)
        } else {
            if self.is_zero() {
                return Err(Error::DivisionByZero);
            }
            self.native.as_ref().clone().inv().pow(magnitude)
        };
        Self::from_native(self.ctx.clone(), native)
    }

    pub fn derivative(&self, variable: usize) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        Self::from_native(self.ctx.clone(), self.native.derivative(variable))
    }

    /// Substitute one variable by an exact rational number.
    ///
    /// Symbolica's integer-backed `RationalPolynomial` has mapped full
    /// evaluation but no partial rational replacement. Lift its two native
    /// integer polynomials to `Q`, call the typed polynomial `replace`, and
    /// normalize directly back to the native representation. The lazy public
    /// `Poly<Q>` compatibility views are never materialized on this path.
    pub fn substitute_rational(&self, variable: usize, value: &Rational) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if value.is_integer() {
            return self.substitute_integer(variable, value.numerator_ref());
        }
        let numerator = lift_native_polynomial(&self.native.numerator).replace(variable, value);
        let denominator = lift_native_polynomial(&self.native.denominator).replace(variable, value);
        if denominator.is_zero() {
            return Err(Error::DivisionByZero);
        }
        Self::from_native(
            self.ctx.clone(),
            NativeRat::from_num_den(numerator, denominator, &Z, true),
        )
    }

    /// Substitute one variable by an exact integer without leaving the native
    /// integer-polynomial representation.
    pub fn substitute_integer(&self, variable: usize, value: &Integer) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        let numerator = self.native.numerator.replace(variable, value);
        let denominator = self.native.denominator.replace(variable, value);
        if denominator.is_zero() {
            return Err(Error::DivisionByZero);
        }
        Self::from_native(
            self.ctx.clone(),
            NativeRat::from_num_den(numerator, denominator, &Z, true),
        )
    }

    /// Evaluate at a complete exact rational point.
    pub fn evaluate_rational(&self, values: &[Rational]) -> Result<Rational> {
        if values.len() != self.ctx.len() {
            return Err(Error::InvalidInput(format!(
                "expected {} evaluation values, got {}",
                self.ctx.len(),
                values.len()
            )));
        }
        let map_integer = |coefficient: &Integer| Q.to_element_numerator(coefficient.clone());
        let numerator = self
            .native
            .numerator
            .evaluate_with_coeff_map(map_integer, values, &Q);
        let denominator = self
            .native
            .denominator
            .evaluate_with_coeff_map(map_integer, values, &Q);
        if Q.is_zero(&denominator) {
            return Err(Error::DivisionByZero);
        }
        Ok(Q.div(&numerator, &denominator))
    }

    /// Evaluate at a complete exact integer point using native integer
    /// polynomial evaluation before constructing the final rational value.
    pub fn evaluate_integer(&self, values: &[Integer]) -> Result<Rational> {
        if values.len() != self.ctx.len() {
            return Err(Error::InvalidInput(format!(
                "expected {} evaluation values, got {}",
                self.ctx.len(),
                values.len()
            )));
        }
        let numerator = self.native.numerator.replace_all(values);
        let denominator = self.native.denominator.replace_all(values);
        if Z.is_zero(&denominator) {
            return Err(Error::DivisionByZero);
        }
        Ok(Q.to_element(numerator, denominator, true))
    }

    /// Integrate a rational function known to be polynomial in `variable`.
    ///
    /// This is the polynomial-part kernel used by the hyperlog primitive. It
    /// calls Symbolica's native polynomial `integrate` after one integer-to-Q
    /// coefficient lift and never materializes the lazy compatibility views.
    pub(crate) fn integrate_polynomial_part(&self, variable: usize) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if self.native.denominator.degree(variable) > 0 {
            return Err(Error::InvalidInput(
                "partial-fraction polynomial part has a variable-dependent denominator".into(),
            ));
        }
        if self.native.numerator.degree(variable) == u16::MAX {
            return Err(Error::InvalidInput(format!(
                "polynomial exponent overflow while integrating variable `{}`",
                self.ctx.vars()[variable]
            )));
        }
        let numerator = lift_native_polynomial(&self.native.numerator).integrate(variable);
        let denominator = lift_native_polynomial(&self.native.denominator);
        Self::from_native(
            self.ctx.clone(),
            NativeRat::from_num_den(numerator, denominator, &Z, true),
        )
    }

    /// Signed Laurent order at `variable = 0`; `i64::MAX` denotes zero.
    pub fn pole_degree(&self, variable: usize) -> Result<i64> {
        if self.is_zero() {
            return Ok(i64::MAX);
        }
        Ok(self.numerator().min_exponent(variable)? - self.denominator().min_exponent(variable)?)
    }

    /// Leading Laurent coefficient at `variable = 0`.
    pub fn residue(&self, variable: usize) -> Result<Self> {
        if self.is_zero() {
            return Ok(Self::zero(self.ctx().clone()));
        }
        let numerator_degree = self.numerator().min_exponent(variable)?;
        let denominator_degree = self.denominator().min_exponent(variable)?;
        Self::new(
            self.numerator()
                .coefficient_of(variable, numerator_degree)?,
            self.denominator()
                .coefficient_of(variable, denominator_degree)?,
        )
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Arc<PolyCtx> {
        PolyCtx::new(["x", "y"]).unwrap()
    }

    #[test]
    fn parse_and_cancel() {
        let ctx = context();
        let rational = Rat::parse(ctx.clone(), "(x^2-y^2)/(x-y)").unwrap();
        assert_eq!(rational, Rat::parse(ctx, "x+y").unwrap());
    }

    #[test]
    fn arithmetic_stays_canonical() {
        let ctx = context();
        let a = Rat::parse(ctx.clone(), "1/x").unwrap();
        let b = Rat::parse(ctx.clone(), "1/y").unwrap();
        assert_eq!(&a + &b, Rat::parse(ctx.clone(), "(x+y)/(x*y)").unwrap());
        assert_eq!(&a * &b, Rat::parse(ctx, "1/(x*y)").unwrap());
    }

    #[test]
    fn native_value_is_shared_and_q_views_are_lazy_and_monic() {
        let ctx = context();
        let rational = Rat::parse(ctx.clone(), "(x+1)/(2*y+2)").unwrap();
        let cloned = rational.clone();

        assert!(Arc::ptr_eq(&rational.native, &cloned.native));
        assert!(Arc::ptr_eq(&rational.views, &cloned.views));
        assert!(rational.views.get().is_none());

        assert_eq!(
            rational.numerator(),
            &Poly::parse(ctx.clone(), "x/2+1/2").unwrap()
        );
        assert_eq!(rational.denominator(), &Poly::parse(ctx, "y+1").unwrap());
        assert!(cloned.views.get().is_some());
    }

    #[test]
    fn constructor_delegates_content_and_polynomial_cancellation_to_symbolica() {
        let ctx = context();
        let numerator = Poly::parse(ctx.clone(), "3/10*(x^2-y^2)").unwrap();
        let denominator = Poly::parse(ctx.clone(), "9/14*(x-y)").unwrap();
        let rational = Rat::new(numerator, denominator).unwrap();

        assert_eq!(rational, Rat::parse(ctx.clone(), "7/15*(x+y)").unwrap());
        assert!(rational.native.denominator.is_constant());
        assert!(!rational.native.denominator.lcoeff().is_negative());
        assert_eq!(
            Rat::from_atom(ctx, rational.to_atom().as_view()).unwrap(),
            rational
        );
    }

    #[test]
    fn every_arithmetic_kernel_matches_exact_symbolica_normalization() {
        let ctx = context();
        let left = Rat::parse(ctx.clone(), "(x^3+2*x*y-y)/(x^2-y^2)").unwrap();
        let right = Rat::parse(ctx.clone(), "(x-y+3)/(x*y+y^2)").unwrap();

        assert_eq!(
            left.try_add(&right).unwrap(),
            Rat::from_atom(ctx.clone(), (left.to_atom() + right.to_atom()).as_view()).unwrap()
        );
        assert_eq!(
            left.try_mul(&right).unwrap(),
            Rat::from_atom(ctx.clone(), (left.to_atom() * right.to_atom()).as_view()).unwrap()
        );
        assert_eq!(
            left.try_div(&right).unwrap(),
            Rat::from_atom(ctx.clone(), (left.to_atom() / right.to_atom()).as_view()).unwrap()
        );
        assert_eq!(
            left.try_sub(&right).unwrap(),
            Rat::from_atom(ctx.clone(), (left.to_atom() - right.to_atom()).as_view()).unwrap()
        );
        assert_eq!(
            left.pow(13).unwrap(),
            Rat::from_atom(ctx, left.to_atom().pow(13).as_view()).unwrap()
        );
    }

    #[test]
    fn large_shared_denominator_addition_cancels_once_and_stays_canonical() {
        let ctx = PolyCtx::new(["x", "y", "z"]).unwrap();
        let common = "(x+y+1)^8*(z+x+2)^6";
        let left = Rat::parse(ctx.clone(), &format!("(x^7+y^5*z+3*x*y+1)/({common})")).unwrap();
        let right = Rat::parse(ctx.clone(), &format!("(y^7-x^4*z+2*x*z+5)/({common})")).unwrap();
        let sum = left.try_add(&right).unwrap();
        let expected = Rat::parse(
            ctx,
            &format!("(x^7+y^7+y^5*z-x^4*z+3*x*y+2*x*z+6)/({common})"),
        )
        .unwrap();

        assert_eq!(sum, expected);
        assert!(!sum.native.denominator.lcoeff().is_negative());
    }

    #[test]
    fn context_and_domain_errors_remain_typed() {
        let xy = context();
        let yx = PolyCtx::new(["y", "x"]).unwrap();
        let left = Rat::parse(xy.clone(), "x/y").unwrap();
        let reordered = Rat::parse(yx, "x/y").unwrap();

        assert!(matches!(
            left.try_add(&reordered),
            Err(Error::ContextMismatch)
        ));
        assert!(matches!(
            Rat::new(Poly::one(xy.clone()), Poly::zero(xy.clone())),
            Err(Error::DivisionByZero)
        ));
        assert!(matches!(
            left.try_div(&Rat::zero(xy.clone())),
            Err(Error::DivisionByZero)
        ));
        assert!(matches!(
            Rat::zero(xy.clone()).pow(-1),
            Err(Error::DivisionByZero)
        ));
        assert!(matches!(
            left.pow(i64::MIN),
            Err(Error::InvalidExponent(i64::MIN))
        ));
        assert!(matches!(left.derivative(2), Err(Error::UnknownVariable(_))));
        assert!(Rat::parse(xy, "z").is_err());
    }

    #[test]
    fn negative_power_and_typed_native_substitution_are_exact() {
        let ctx = context();
        let rational = Rat::parse(ctx.clone(), "(x+2*y)/(x-y)").unwrap();
        assert_eq!(
            rational.pow(-3).unwrap(),
            Rat::parse(ctx.clone(), "(x-y)^3/(x+2*y)^3").unwrap()
        );
        let substituted = rational
            .substitute_rational(0, &Rational::new(1, 2))
            .unwrap();
        assert_eq!(
            substituted,
            Rat::parse(ctx.clone(), "(1+4*y)/(1-2*y)").unwrap()
        );
        assert_eq!(
            rational.substitute_integer(0, &Integer::from(2)).unwrap(),
            Rat::parse(ctx.clone(), "(2+2*y)/(2-y)").unwrap()
        );
        assert!(rational.views.get().is_none());
        assert!(substituted.views.get().is_none());
    }

    #[test]
    fn rational_and_integer_evaluation_are_exact_and_check_poles() {
        let ctx = context();
        let rational = Rat::parse(ctx.clone(), "(x+2*y)/(x-y)").unwrap();
        assert_eq!(
            rational
                .evaluate_rational(&[Rational::new(1, 2), Rational::new(1, 3)])
                .unwrap(),
            Rational::from(7)
        );
        assert_eq!(
            rational
                .evaluate_integer(&[Integer::from(2), Integer::from(1)])
                .unwrap(),
            Rational::from(4)
        );
        assert!(matches!(
            rational.evaluate_rational(&[Rational::one(), Rational::one()]),
            Err(Error::DivisionByZero)
        ));
        assert!(matches!(
            rational.evaluate_integer(&[Integer::from(1)]),
            Err(Error::InvalidInput(_))
        ));
        assert!(rational.views.get().is_none());

        let zero = Rat::zero(ctx);
        assert_eq!(
            zero.evaluate_integer(&[Integer::from(13), Integer::from(-8)])
                .unwrap(),
            Rational::zero()
        );
        assert!(
            zero.substitute_rational(1, &Rational::new(5, 9))
                .unwrap()
                .is_zero()
        );
        assert!(zero.views.get().is_none());
    }

    #[test]
    fn native_substitution_detects_zero_denominators_without_materializing_views() {
        let ctx = context();
        let rational = Rat::parse(ctx, "(x+y)/(x-1)").unwrap();
        assert!(matches!(
            rational.substitute_rational(0, &Rational::one()),
            Err(Error::DivisionByZero)
        ));
        assert!(matches!(
            rational.substitute_integer(0, &Integer::from(1)),
            Err(Error::DivisionByZero)
        ));
        assert!(matches!(
            rational.substitute_integer(2, &Integer::from(1)),
            Err(Error::UnknownVariable(_))
        ));
        assert!(rational.views.get().is_none());
    }

    #[test]
    fn sparse_substitution_commutes_with_full_evaluation() {
        let ctx = context();
        let expressions = [
            "(x^17+2*y^9+1)/(x^2+y^2+1)",
            "(3*x*y-2)/(x+y+5)",
            "(x^31-y^13)/(2*x^2+3*y^2+1)",
        ];
        let points = [
            (Rational::new(2, 3), Rational::new(-1, 2)),
            (Rational::from(3), Rational::from(2)),
        ];

        for expression in expressions {
            let rational = Rat::parse(ctx.clone(), expression).unwrap();
            for (x, y) in &points {
                let direct = rational.evaluate_rational(&[x.clone(), y.clone()]).unwrap();
                let after_substitution = rational
                    .substitute_rational(0, x)
                    .unwrap()
                    .evaluate_rational(&[Rational::zero(), y.clone()])
                    .unwrap();
                assert_eq!(after_substitution, direct, "{expression} at ({x}, {y})");
            }
        }
    }

    #[test]
    fn derivative_and_laurent_residue() {
        let ctx = context();
        let rational = Rat::parse(ctx.clone(), "(1+x)/(x^2*y)").unwrap();
        assert_eq!(rational.pole_degree(0).unwrap(), -2);
        assert_eq!(
            rational.residue(0).unwrap(),
            Rat::parse(ctx.clone(), "1/y").unwrap()
        );
        assert_eq!(
            rational.derivative(0).unwrap(),
            Rat::parse(ctx, "(-x-2)/(x^3*y)").unwrap()
        );
    }

    #[test]
    fn native_polynomial_part_integral_round_trips_without_q_views() {
        let ctx = context();
        let rational = Rat::parse(ctx.clone(), "(x^20+3*x^2+1)/(y+1)").unwrap();
        let primitive = rational.integrate_polynomial_part(0).unwrap();

        assert_eq!(primitive.derivative(0).unwrap(), rational);
        assert_eq!(
            primitive,
            Rat::parse(ctx.clone(), "(x^21/21+x^3+x)/(y+1)").unwrap()
        );
        assert!(rational.views.get().is_none());
        assert!(primitive.views.get().is_none());
        assert!(matches!(
            Rat::parse(ctx, "1/(x+1)")
                .unwrap()
                .integrate_polynomial_part(0),
            Err(Error::InvalidInput(_))
        ));
    }
}
