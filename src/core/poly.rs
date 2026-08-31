use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::{Add, Mul, Neg, Sub};
use std::sync::Arc;

use symbolica::domains::rational::RationalField;
use symbolica::prelude::*;

use crate::error::{Error, Result};
use crate::symbols::SYMBOL_NAMESPACE;

type SymbolicaPoly = MultivariatePolynomial<RationalField, u16>;

/// The ordered variable set shared by polynomials in one exact ring.
#[derive(Clone, Debug)]
pub struct PolyCtx {
    names: Arc<Vec<String>>,
    variables: Arc<Vec<PolyVariable>>,
}

impl PolyCtx {
    pub fn new<I, S>(variables: I) -> Result<Arc<Self>>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let names = variables.into_iter().map(Into::into).collect::<Vec<_>>();
        let mut seen = std::collections::HashSet::with_capacity(names.len());
        for name in &names {
            if name.is_empty() || !seen.insert(name.clone()) {
                return Err(Error::InvalidInput(format!(
                    "variable names must be non-empty and unique: `{name}`"
                )));
            }
        }

        let variables = names
            .iter()
            .map(|name| {
                Symbol::parse(name, SYMBOL_NAMESPACE)
                    .map(PolyVariable::Symbol)
                    .map_err(|message| Error::PolynomialParse {
                        expression: name.clone(),
                        message,
                    })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Arc::new(Self {
            names: Arc::new(names),
            variables: Arc::new(variables),
        }))
    }

    /// Build a polynomial context directly from Symbolica symbols.
    ///
    /// This is the primary construction path for the Atom-native API. String
    /// names are retained only for diagnostics and transport adapters.
    pub fn from_symbols<I>(symbols: I) -> Result<Arc<Self>>
    where
        I: IntoIterator<Item = Symbol>,
    {
        let symbols = symbols.into_iter().collect::<Vec<_>>();
        let mut seen = std::collections::HashSet::with_capacity(symbols.len());
        for symbol in &symbols {
            if !seen.insert(*symbol) {
                return Err(Error::InvalidInput(format!(
                    "polynomial symbols must be unique: `{}`",
                    symbol.get_name()
                )));
            }
        }
        let names = symbols
            .iter()
            .map(|symbol| symbol.get_name().to_owned())
            .collect::<Vec<_>>();
        let variables = symbols
            .into_iter()
            .map(PolyVariable::Symbol)
            .collect::<Vec<_>>();
        Ok(Arc::new(Self {
            names: Arc::new(names),
            variables: Arc::new(variables),
        }))
    }

    /// Build a context from arbitrary Symbolica polynomial indeterminates.
    ///
    /// Besides ordinary symbols, Symbolica can treat function calls and
    /// non-polynomial powers as atomic polynomial variables. This constructor
    /// preserves that native representation for Atom-based callers.
    pub fn from_indeterminates<I>(indeterminates: I) -> Result<Arc<Self>>
    where
        I: IntoIterator<Item = Atom>,
    {
        let variables = indeterminates
            .into_iter()
            .map(|atom| PolyVariable::try_from(atom).map_err(Error::InvalidInput))
            .collect::<Result<Vec<_>>>()?;
        let mut seen = std::collections::HashSet::with_capacity(variables.len());
        for variable in &variables {
            if !seen.insert(variable.clone()) {
                return Err(Error::InvalidInput(format!(
                    "polynomial indeterminates must be unique: `{variable}`"
                )));
            }
        }
        let names = variables
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        Ok(Arc::new(Self {
            names: Arc::new(names),
            variables: Arc::new(variables),
        }))
    }

    pub fn vars(&self) -> &[String] {
        self.names.as_slice()
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|candidate| candidate == name)
    }

    pub fn index_of_symbol(&self, symbol: Symbol) -> Option<usize> {
        self.variables
            .iter()
            .position(|variable| variable == &symbol)
    }

    /// Locate an arbitrary Symbolica polynomial indeterminate structurally.
    pub fn index_of_indeterminate(&self, atom: AtomView<'_>) -> Option<usize> {
        let variable = PolyVariable::try_from(atom.to_owned()).ok()?;
        self.variables
            .iter()
            .position(|candidate| candidate == &variable)
    }

    pub fn variable_atom(&self, index: usize) -> Result<Atom> {
        self.variables
            .get(index)
            .cloned()
            .map(Atom::from)
            .ok_or_else(|| Error::UnknownVariable(index.to_string()))
    }

    pub(crate) fn variable_map(&self) -> Arc<Vec<PolyVariable>> {
        self.variables.clone()
    }

    /// Return the context variables as Symbolica symbols.
    pub fn symbol_variables(&self) -> Result<Vec<Symbol>> {
        self.variables
            .iter()
            .map(|variable| match variable {
                PolyVariable::Symbol(symbol) => Ok(*symbol),
                _ => Err(Error::InvalidInput(
                    "polynomial context contains a non-symbol indeterminate".into(),
                )),
            })
            .collect()
    }

    fn compatible_with(&self, other: &Self) -> bool {
        self.names == other.names && self.variables == other.variables
    }
}

/// An exact multivariate polynomial over the rationals.
///
/// Symbolica owns the sparse canonical representation.  `Poly` only adds the
/// fixed variable context and the compatibility operations used by HyperFLINT.
#[derive(Clone, Debug)]
pub struct Poly {
    ctx: Arc<PolyCtx>,
    inner: SymbolicaPoly,
}

impl Poly {
    pub fn zero(ctx: Arc<PolyCtx>) -> Self {
        let inner = SymbolicaPoly::new(&Q, None, ctx.variable_map());
        Self { ctx, inner }
    }

    pub fn one(ctx: Arc<PolyCtx>) -> Self {
        let inner = SymbolicaPoly::new(&Q, Some(1), ctx.variable_map()).one();
        Self { ctx, inner }
    }

    pub fn from_int(ctx: Arc<PolyCtx>, value: i64) -> Self {
        let template = SymbolicaPoly::new(&Q, Some(1), ctx.variable_map());
        let inner = template.constant(Q.nth(value.into()));
        Self { ctx, inner }
    }

    pub fn generator(ctx: Arc<PolyCtx>, variable: usize) -> Result<Self> {
        let symbol = ctx
            .variables
            .get(variable)
            .ok_or_else(|| Error::UnknownVariable(variable.to_string()))?
            .clone();
        let template = SymbolicaPoly::new(&Q, Some(1), ctx.variable_map());
        let inner = template.variable(&symbol).map_err(Error::InvalidInput)?;
        Ok(Self { ctx, inner })
    }

    pub fn parse(ctx: Arc<PolyCtx>, expression: &str) -> Result<Self> {
        let atom = Atom::parse(expression, SYMBOL_NAMESPACE, ParseSettings::default()).map_err(
            |message| Error::PolynomialParse {
                expression: expression.to_owned(),
                message,
            },
        )?;
        Self::from_atom(ctx, atom.as_view()).map_err(|error| Error::PolynomialParse {
            expression: expression.to_owned(),
            message: error.to_string(),
        })
    }

    /// Convert a Symbolica atom into the context's exact polynomial ring.
    pub fn from_atom(ctx: Arc<PolyCtx>, atom: AtomView<'_>) -> Result<Self> {
        let inner = atom
            .try_to_polynomial::<_, u16>(&Q, Some(ctx.variable_map()))
            .map_err(|error| Error::InvalidInput(error.to_string()))?;
        if inner.get_vars_ref() != ctx.variables.as_slice() {
            return Err(Error::InvalidInput(
                "atom contains a symbol outside the polynomial context".into(),
            ));
        }
        Ok(Self { ctx, inner })
    }

    /// Convert the exact polynomial back to a normalized Symbolica atom.
    pub fn to_atom(&self) -> Atom {
        self.inner.to_expression()
    }

    pub(crate) fn from_inner(ctx: Arc<PolyCtx>, inner: SymbolicaPoly) -> Self {
        Self { ctx, inner }
    }

    pub(crate) fn inner(&self) -> &SymbolicaPoly {
        &self.inner
    }

    pub fn ctx(&self) -> &Arc<PolyCtx> {
        &self.ctx
    }

    pub fn is_zero(&self) -> bool {
        self.inner.is_zero()
    }

    pub fn is_one(&self) -> bool {
        self.inner.is_one()
    }

    pub fn is_rational_constant(&self) -> bool {
        self.inner.is_constant()
    }

    /// Return the exact value when this polynomial is constant.
    pub fn rational_constant(&self) -> Option<Rational> {
        self.is_rational_constant()
            .then(|| self.inner.get_constant())
    }

    pub fn n_terms(&self) -> usize {
        self.inner.nterms()
    }

    pub fn total_degree(&self) -> u64 {
        self.inner
            .exponents_iter()
            .map(|exponents| exponents.iter().map(|&value| u64::from(value)).sum())
            .max()
            .unwrap_or(0)
    }

    pub fn equal(&self, other: &Self) -> bool {
        self.ctx.compatible_with(&other.ctx) && self.inner == other.inner
    }

    pub fn leading_coefficient_is_negative(&self) -> bool {
        !self.is_zero() && self.inner.lcoeff().numerator_ref().is_negative()
    }

    pub fn used_variable_indices(&self) -> Vec<usize> {
        (0..self.ctx.len())
            .filter(|&variable| self.inner.contains(variable))
            .collect()
    }

    pub fn transplant(
        &self,
        destination: Arc<PolyCtx>,
        source_to_destination: &[Option<usize>],
    ) -> Result<Self> {
        if source_to_destination.len() != self.ctx.len() {
            return Err(Error::InvalidInput(
                "transplant map length differs from source context".into(),
            ));
        }

        for &target in source_to_destination.iter().flatten() {
            if target >= destination.len() {
                return Err(Error::InvalidInput(format!(
                    "destination variable index {target} is out of range"
                )));
            }
        }

        // Symbolica's native rearrangement is the fastest exact path when the
        // map is induced by structural variable identity. It supports
        // permutations, context growth, and dropping unused variables.
        let identity_map = self
            .ctx
            .variables
            .iter()
            .map(|source| {
                destination
                    .variables
                    .iter()
                    .position(|target| target == source)
            })
            .collect::<Vec<_>>();
        if identity_map == source_to_destination {
            let inner = self
                .inner
                .rearrange_with_growth(destination.variables.as_slice())
                .map_err(Error::InvalidInput)?;
            return Ok(Self::from_inner(destination, inner));
        }

        // An explicit map may rename variables or identify several source
        // variables with one destination variable. Native `rearrange` does
        // not express those semantics, so retain the sparse term adapter and
        // merge powers (and any colliding monomials) exactly.
        let mut inner = SymbolicaPoly::new(&Q, Some(self.n_terms()), destination.variable_map());
        for (term, coefficient) in self
            .inner
            .exponents_iter()
            .zip(self.inner.coefficients.iter())
        {
            let mut exponents = vec![0_u16; destination.len()];
            for (source, &exponent) in term.iter().enumerate() {
                if exponent == 0 {
                    continue;
                }
                let target = source_to_destination[source].ok_or_else(|| {
                    Error::InvalidInput(format!(
                        "used source variable `{}` has no destination",
                        self.ctx.vars()[source]
                    ))
                })?;
                exponents[target] = exponents[target].checked_add(exponent).ok_or_else(|| {
                    Error::InvalidInput(format!(
                        "transplant exponent overflow at destination variable `{}`",
                        destination.vars()[target]
                    ))
                })?;
            }
            inner.append_monomial(coefficient.clone(), &exponents);
        }
        Ok(Self::from_inner(destination, inner))
    }

    fn require_same_context(&self, other: &Self) -> Result<()> {
        if self.ctx.compatible_with(&other.ctx) {
            Ok(())
        } else {
            Err(Error::ContextMismatch)
        }
    }

    pub fn try_add(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        Ok(Self::from_inner(
            self.ctx.clone(),
            &self.inner + &other.inner,
        ))
    }

    pub fn try_sub(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        Ok(Self::from_inner(
            self.ctx.clone(),
            &self.inner - &other.inner,
        ))
    }

    pub fn try_mul(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        Ok(Self::from_inner(
            self.ctx.clone(),
            &self.inner * &other.inner,
        ))
    }

    pub fn pow(&self, exponent: usize) -> Self {
        Self::from_inner(self.ctx.clone(), self.inner.pow(exponent))
    }

    pub fn derivative(&self, variable: usize) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        Ok(Self::from_inner(
            self.ctx.clone(),
            self.inner.derivative(variable),
        ))
    }

    /// Substitute one variable by an exact rational number.
    pub fn substitute_rational(&self, variable: usize, value: &Rational) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        Ok(Self::from_inner(
            self.ctx.clone(),
            self.inner.replace(variable, value),
        ))
    }

    /// Substitute one variable by an exact integer.
    pub fn substitute_integer(&self, variable: usize, value: &Integer) -> Result<Self> {
        self.substitute_rational(variable, &Rational::from(value))
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
        Ok(self.inner.replace_all(values))
    }

    /// Evaluate at a complete exact integer point.
    pub fn evaluate_integer(&self, values: &[Integer]) -> Result<Rational> {
        let values = values.iter().map(Rational::from).collect::<Vec<_>>();
        self.evaluate_rational(&values)
    }

    /// Integrate with respect to one variable, choosing zero integration
    /// constant. Symbolica performs the coefficient divisions in `Q`.
    pub fn integrate(&self, variable: usize) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if self.degree(variable)? == i64::from(u16::MAX) {
            return Err(Error::InvalidInput(format!(
                "polynomial exponent overflow while integrating variable `{}`",
                self.ctx.vars()[variable]
            )));
        }
        Ok(Self::from_inner(
            self.ctx.clone(),
            self.inner.integrate(variable),
        ))
    }

    pub fn div_exact(&self, divisor: &Self) -> Result<Self> {
        self.require_same_context(divisor)?;
        if divisor.is_zero() {
            return Err(Error::DivisionByZero);
        }
        self.inner
            .try_div_exact(&divisor.inner)
            .map(|inner| Self::from_inner(self.ctx.clone(), inner))
            .ok_or(Error::InexactDivision)
    }

    pub fn divides(&self, dividend: &Self) -> Result<bool> {
        self.require_same_context(dividend)?;
        if self.is_zero() {
            return Ok(dividend.is_zero());
        }
        Ok(dividend.inner.try_div_exact(&self.inner).is_some())
    }

    pub fn div_rem(&self, divisor: &Self) -> Result<(Self, Self)> {
        self.require_same_context(divisor)?;
        if divisor.is_zero() {
            return Err(Error::DivisionByZero);
        }
        let (quotient, remainder) = self.inner.quot_rem(&divisor.inner, false);
        Ok((
            Self::from_inner(self.ctx.clone(), quotient),
            Self::from_inner(self.ctx.clone(), remainder),
        ))
    }

    pub fn gcd(&self, other: &Self) -> Result<Self> {
        self.require_same_context(other)?;
        Ok(Self::from_inner(
            self.ctx.clone(),
            self.inner.gcd(&other.inner),
        ))
    }

    pub fn resultant(&self, other: &Self, variable: usize) -> Result<Self> {
        self.require_same_context(other)?;
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        let left = self.inner.to_univariate(variable);
        let right = other.inner.to_univariate(variable);
        Ok(Self::from_inner(self.ctx.clone(), left.resultant(&right)))
    }

    /// HyperFLINT's historical `Res(p, p') / lc(p)` convention.
    ///
    /// For even degrees this differs by a sign from the standard
    /// discriminant. The JSON compatibility bridge deliberately exposes this
    /// legacy operation; new Rust callers should use [`Self::discriminant`].
    pub fn resultant_discriminant(&self, variable: usize) -> Result<Self> {
        if self.degree(variable)? < 1 {
            return Ok(Self::one(self.ctx.clone()));
        }
        let derivative = self.derivative(variable)?;
        let resultant = self.resultant(&derivative, variable)?;
        let leading = Self::from_inner(self.ctx.clone(), self.inner.univariate_lcoeff(variable));
        resultant.div_exact(&leading)
    }

    /// Standard polynomial discriminant
    /// `(-1)^(n(n-1)/2) Res(p,p') / lc(p)`.
    pub fn discriminant(&self, variable: usize) -> Result<Self> {
        let degree = self.degree(variable)?;
        let legacy = self.resultant_discriminant(variable)?;
        if degree >= 1 && (degree * (degree - 1) / 2) % 2 != 0 {
            Ok(-&legacy)
        } else {
            Ok(legacy)
        }
    }

    pub fn degree(&self, variable: usize) -> Result<i64> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if self.is_zero() {
            Ok(-1)
        } else {
            Ok(i64::from(self.inner.degree(variable)))
        }
    }

    pub fn min_exponent(&self, variable: usize) -> Result<i64> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if self.is_zero() {
            Ok(i64::MAX)
        } else {
            Ok(i64::from(self.inner.degree_bounds(variable).0))
        }
    }

    pub fn coefficient_of(&self, variable: usize, exponent: i64) -> Result<Self> {
        if variable >= self.ctx.len() {
            return Err(Error::UnknownVariable(variable.to_string()));
        }
        if !(0..=u16::MAX as i64).contains(&exponent) {
            return Ok(Self::zero(self.ctx.clone()));
        }
        let exponent = exponent as u16;
        let mut inner = SymbolicaPoly::new(&Q, None, self.ctx.variable_map());
        for (term, coefficient) in self
            .inner
            .exponents_iter()
            .zip(self.inner.coefficients.iter())
        {
            if term[variable] != exponent {
                continue;
            }
            let mut output_exponents = term.to_vec();
            output_exponents[variable] = 0;
            inner.append_monomial(coefficient.clone(), &output_exponents);
        }
        Ok(Self::from_inner(self.ctx.clone(), inner))
    }

    pub fn scalar_div(&self, scalar: &Self) -> Result<Self> {
        self.require_same_context(scalar)?;
        if !scalar.is_rational_constant() {
            return Err(Error::InvalidInput("scalar divisor is not constant".into()));
        }
        let coefficient = scalar.inner.get_constant();
        if Q.is_zero(&coefficient) {
            return Err(Error::DivisionByZero);
        }
        Ok(Self::from_inner(
            self.ctx.clone(),
            self.inner.clone().div_coeff(&coefficient),
        ))
    }

    pub fn canonical_proportional_form(&self) -> Self {
        if self.is_zero() {
            return Self::zero(self.ctx.clone());
        }
        let leading = self.inner.lcoeff();
        Self::from_inner(self.ctx.clone(), self.inner.clone().div_coeff(&leading))
    }

    pub fn factor(&self) -> Factored {
        if self.is_zero() {
            return Factored {
                constant: "0".into(),
                factors: Vec::new(),
            };
        }

        let mut constant = Q.one();
        let mut factors = Vec::new();
        for (factor, exponent) in self.inner.factor() {
            if factor.is_constant() {
                constant = Q.mul(&constant, &Q.pow(&factor.get_constant(), exponent as u64));
            } else {
                factors.push((Self::from_inner(self.ctx.clone(), factor), exponent));
            }
        }
        factors.sort_by(|(left, _), (right, _)| {
            left.total_degree()
                .cmp(&right.total_degree())
                .then_with(|| left.to_string().cmp(&right.to_string()))
        });
        Factored {
            constant: constant.to_string(),
            factors,
        }
    }
}

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

/// Factorization with a rational unit and irreducible polynomial bases.
#[derive(Clone, Debug)]
pub struct Factored {
    pub constant: String,
    pub factors: Vec<(Poly, usize)>,
}

#[cfg(test)]
mod tests {
    use std::hash::{DefaultHasher, Hash, Hasher};

    use super::*;

    fn context() -> Arc<PolyCtx> {
        PolyCtx::new(["x", "y"]).unwrap()
    }

    #[test]
    fn arithmetic_and_exact_division() {
        let ctx = context();
        let x_plus_y = Poly::parse(ctx.clone(), "x+y").unwrap();
        let x_minus_y = Poly::parse(ctx.clone(), "x-y").unwrap();
        let product = &x_plus_y * &x_minus_y;
        let expected = Poly::parse(ctx, "x^2-y^2").unwrap();
        assert_eq!(product, expected);
        assert_eq!(product.div_exact(&x_plus_y).unwrap(), x_minus_y);
    }

    #[test]
    fn structural_hash_matches_polynomial_and_context_equality() {
        let first_ctx = context();
        let equivalent_ctx = context();
        let expanded = Poly::parse(first_ctx, "(x+y)^4").unwrap();
        let canonical = Poly::parse(equivalent_ctx, "x^4+4*x^3*y+6*x^2*y^2+4*x*y^3+y^4").unwrap();
        assert_eq!(expanded, canonical);

        let hash = |polynomial: &Poly| {
            let mut state = DefaultHasher::new();
            polynomial.hash(&mut state);
            state.finish()
        };
        assert_eq!(hash(&expanded), hash(&canonical));

        let different_context = PolyCtx::new(["y", "x"]).unwrap();
        let constant_left = Poly::from_int(expanded.ctx().clone(), 3);
        let constant_right = Poly::from_int(different_context, 3);
        assert_ne!(constant_left, constant_right);
    }

    #[test]
    fn improved_symbolica_resultant_is_used() {
        let ctx = context();
        let left = Poly::parse(ctx.clone(), "x^2+y*x+1").unwrap();
        let right = Poly::parse(ctx.clone(), "x-y").unwrap();
        let resultant = left.resultant(&right, 0).unwrap();
        assert_eq!(resultant, Poly::parse(ctx, "2*y^2+1").unwrap());
    }

    #[test]
    fn coefficients_and_discriminant() {
        let ctx = context();
        let polynomial = Poly::parse(ctx.clone(), "y*x^2+3*x+1").unwrap();
        assert_eq!(
            polynomial.coefficient_of(0, 2).unwrap(),
            Poly::parse(ctx.clone(), "y").unwrap()
        );
        assert_eq!(
            polynomial.discriminant(0).unwrap(),
            Poly::parse(ctx.clone(), "9-4*y").unwrap()
        );
        assert_eq!(
            polynomial.resultant_discriminant(0).unwrap(),
            Poly::parse(ctx, "4*y-9").unwrap()
        );
    }

    #[test]
    fn standard_discriminant_has_the_degree_dependent_sign() {
        let ctx = context();
        let cubic = Poly::parse(ctx.clone(), "x^3+y*x+1").unwrap();
        assert_eq!(
            cubic.discriminant(0).unwrap(),
            Poly::parse(ctx.clone(), "-4*y^3-27").unwrap()
        );
        assert_eq!(
            cubic.resultant_discriminant(0).unwrap(),
            Poly::parse(ctx, "4*y^3+27").unwrap()
        );
    }

    #[test]
    fn typed_substitution_and_full_evaluation_are_exact() {
        let ctx = context();
        let polynomial = Poly::parse(ctx.clone(), "3*x^9*y^4-2*x^2*y+7").unwrap();
        let half = Rational::new(1, 2);

        assert_eq!(
            polynomial.substitute_rational(0, &half).unwrap(),
            Poly::parse(ctx.clone(), "3/512*y^4-y/2+7").unwrap()
        );
        assert_eq!(
            polynomial
                .substitute_integer(1, &Integer::from(-2))
                .unwrap(),
            Poly::parse(ctx.clone(), "48*x^9+4*x^2+7").unwrap()
        );
        assert_eq!(
            polynomial
                .evaluate_rational(&[half, Rational::new(1, 3)])
                .unwrap(),
            Rational::new(94_465, 13_824)
        );
        assert_eq!(
            polynomial
                .evaluate_integer(&[Integer::from(2), Integer::from(-1)])
                .unwrap(),
            Rational::from(1_551)
        );

        assert!(matches!(
            polynomial.substitute_rational(2, &Rational::one()),
            Err(Error::UnknownVariable(_))
        ));
        assert!(matches!(
            polynomial.evaluate_rational(&[Rational::one()]),
            Err(Error::InvalidInput(_))
        ));

        let zero = Poly::zero(ctx);
        assert!(
            zero.substitute_integer(0, &Integer::from(37))
                .unwrap()
                .is_zero()
        );
        assert_eq!(
            zero.evaluate_rational(&[Rational::new(2, 3), Rational::new(-5, 7)])
                .unwrap(),
            Rational::zero()
        );
        assert!(zero.integrate(0).unwrap().is_zero());
    }

    #[test]
    fn native_integral_round_trips_and_sparse_coefficients_stay_sparse() {
        let ctx = context();
        let polynomial = Poly::parse(ctx.clone(), "11*x^60000*y^7-5*x^41*y+3*x^2+17").unwrap();

        assert_eq!(
            polynomial.coefficient_of(0, 60_000).unwrap(),
            Poly::parse(ctx.clone(), "11*y^7").unwrap()
        );
        assert!(polynomial.coefficient_of(0, 59_999).unwrap().is_zero());
        assert!(polynomial.coefficient_of(0, -1).unwrap().is_zero());

        let primitive = polynomial.integrate(0).unwrap();
        assert_eq!(primitive.derivative(0).unwrap(), polynomial);
        assert_eq!(
            primitive,
            Poly::parse(ctx.clone(), "11/60001*x^60001*y^7-5/42*x^42*y+x^3+17*x",).unwrap()
        );

        let maximum = Poly::parse(ctx, "x^65535").unwrap();
        assert!(matches!(maximum.integrate(0), Err(Error::InvalidInput(_))));
    }

    #[test]
    fn transplant_uses_native_rearrangement_and_preserves_explicit_identification() {
        let source = context();
        let destination = PolyCtx::new(["y", "z", "x"]).unwrap();
        let polynomial = Poly::parse(source.clone(), "x^2*y+3*y+1").unwrap();
        assert_eq!(
            polynomial
                .transplant(destination.clone(), &[Some(2), Some(0)])
                .unwrap(),
            Poly::parse(destination, "x^2*y+3*y+1").unwrap()
        );

        let identified_ctx = PolyCtx::new(["z"]).unwrap();
        let identified = Poly::parse(source, "x*y+x+y").unwrap();
        assert_eq!(
            identified
                .transplant(identified_ctx.clone(), &[Some(0), Some(0)])
                .unwrap(),
            Poly::parse(identified_ctx.clone(), "z^2+2*z").unwrap()
        );
        assert!(matches!(
            identified.transplant(identified_ctx, &[Some(0), None]),
            Err(Error::InvalidInput(_))
        ));
    }
}
