//! Relocated from Symbolica 75f8350, domains/factorized_rational_polynomial/apart.
//! Preserves the original source license; see DISTRIBUTION-LICENSE.md.
//! Partial fractions with factored rational-function Taylor coefficients.

use symbolica::domains::EuclideanDomain;
use symbolica::domains::factorized_rational_polynomial::{
    FactorizedRationalPolynomial, FromNumeratorAndFactorizedDenominator,
};
use symbolica::domains::rational_polynomial::{FromNumeratorAndDenominator, RationalPolynomial};
use symbolica::poly::{
    PositiveExponent, factor::Factorize, gcd::PolynomialGCD, polynomial::MultivariatePolynomial,
};

type Factored<R, E> = FactorizedRationalPolynomial<R, E>;
type Polynomial<R, E> = MultivariatePolynomial<R, E>;
type ApartTerm<R, E> = (Factored<R, E>, Factored<R, E>, usize);

mod linear;
use linear::try_linear;

pub(super) fn to_rational_polynomial<R: EuclideanDomain + PolynomialGCD<E>, E: PositiveExponent>(
    value: &Factored<R, E>,
) -> RationalPolynomial<R, E>
where
    RationalPolynomial<R, E>: FromNumeratorAndDenominator<R, R, E>,
{
    let numerator = value.numerator.clone().mul_coeff(value.numer_coeff.clone());
    let denominator = value.denominators.iter().fold(
        value.numerator.constant(value.denom_coeff.clone()),
        |product, (base, power)| product * &base.pow(*power),
    );
    RationalPolynomial::from_num_den(numerator, denominator, value.numerator.ring(), true)
}

/// Decompose using factored Taylor coefficients, with Symbolica's general fallback.
pub(super) fn apart_factored_denominators<
    R: EuclideanDomain + PolynomialGCD<E>,
    E: PositiveExponent,
>(
    value: &Factored<R, E>,
    var: usize,
) -> Vec<ApartTerm<R, E>>
where
    Factored<R, E>: FromNumeratorAndFactorizedDenominator<R, R, E>,
    RationalPolynomial<R, E>: FromNumeratorAndDenominator<R, R, E>,
    Polynomial<R, E>: Factorize,
{
    assert!(
        var < value.numerator.nvars(),
        "partial fraction variable out of range"
    );
    assert!(
        !value.numerator.ring().is_zero(&value.denom_coeff),
        "zero denominator"
    );
    assert!(
        value
            .denominators
            .iter()
            .all(|(base, power)| *power == 0 || !base.is_zero()),
        "zero denominator"
    );

    // Public fields and the nonfactoring constructor allow distinct maps.
    // Reconstruct without factorization to unify maps and scalar units;
    // the source variable remains at its original index in the numerator.
    let mut denominators = value.denominators.clone();
    denominators.push((value.numerator.constant(value.denom_coeff.clone()), 1));
    let input = Factored::from_num_den(
        value.numerator.clone().mul_coeff(value.numer_coeff.clone()),
        denominators,
        value.numerator.ring(),
        false,
    );
    if let Some(terms) = try_linear(&input, var) {
        return terms;
    }

    to_rational_polynomial(&input)
        .apart_factored_denominators(var)
        .into_iter()
        .map(|(numerator, denominator, exponent)| {
            let from_rational = |value: RationalPolynomial<R, E>| {
                Factored::from_num_den(
                    value.numerator,
                    vec![(value.denominator, 1)],
                    input.numerator.ring(),
                    true,
                )
            };
            (
                from_rational(numerator),
                from_rational(denominator),
                exponent,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests;
