//! Shared FindRoots admissibility and square-root carry/discharge logic.

use symbolica::prelude::*;

use crate::core::Poly;
use crate::error::Result;

use super::structural_keys::PolyIndex;

#[derive(Clone, Debug, Default)]
pub struct FrJudgment {
    pub ok: bool,
    pub carry: Vec<Poly>,
    pub kin: u64,
    pub terminal: u64,
}

/// Path-local deferred square-root state. Sibling DFS branches never share
/// this value.
#[derive(Clone, Debug, Default)]
pub struct PathState {
    pub carried: Vec<Poly>,
    pub nsq: u64,
    pub nkin: u64,
    pub ntq: u64,
    pub nonexec: bool,
    /// Path-wide, proportionality-deduplicated obligation ledger.
    pub minted: Vec<Poly>,
    minted_index: PolyIndex,
}

fn integer_is_square(value: &Integer) -> bool {
    if value.is_negative() {
        return false;
    }
    let root = value.root(2);
    &root * &root == *value
}

fn rational_is_square(value: &Rational) -> bool {
    integer_is_square(value.numerator_ref()) && integer_is_square(value.denominator_ref())
}

fn perfect_square(polynomial: &Poly) -> bool {
    if polynomial.is_zero() {
        return true;
    }
    let content = polynomial.inner().content();
    if !rational_is_square(&content) {
        return false;
    }
    let primitive = polynomial.inner().clone().div_coeff(&content);
    primitive
        .square_free_factorization()
        .into_iter()
        .all(|(factor, multiplicity)| {
            factor.is_one() || (!factor.is_constant() && multiplicity % 2 == 0)
        })
}

fn depends_on_any(polynomial: &Poly, variables: &[usize]) -> Result<bool> {
    for &variable in variables {
        if polynomial.degree(variable)? > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn degauge(polynomial: &Poly, gauge: Option<usize>) -> Result<Poly> {
    match gauge {
        Some(variable) if polynomial.degree(variable)? > 0 => {
            polynomial.substitute_rational(variable, &Rational::one())
        }
        _ => Ok(polynomial.clone()),
    }
}

/// A quadratic is Euler-conic rationalizable iff its leading and constant
/// coefficients in the pivot are rational polynomial squares.
pub fn conic_rationalizable(letter: &Poly, pivot: usize) -> Result<bool> {
    if letter.degree(pivot)? != 2 {
        return Ok(false);
    }
    Ok(perfect_square(&letter.coefficient_of(pivot, 2)?)
        && perfect_square(&letter.coefficient_of(pivot, 0)?))
}

/// FindRoots judgment for one letter.
pub fn fr_judge(
    letter: &Poly,
    pivot: usize,
    pending: &[usize],
    _all_xvars: &[usize],
) -> Result<FrJudgment> {
    let degree = letter.degree(pivot)?;
    if degree <= 1 {
        return Ok(FrJudgment {
            ok: true,
            ..FrJudgment::default()
        });
    }
    if degree >= 3 {
        return Ok(FrJudgment::default());
    }
    if conic_rationalizable(letter, pivot)? {
        return Ok(FrJudgment {
            ok: true,
            ..FrJudgment::default()
        });
    }
    if pending.is_empty() {
        return Ok(FrJudgment {
            ok: true,
            terminal: 1,
            ..FrJudgment::default()
        });
    }

    let discriminant = letter.discriminant(pivot)?;
    if discriminant.is_zero() {
        return Ok(FrJudgment {
            ok: true,
            ..FrJudgment::default()
        });
    }

    let mut judgment = FrJudgment {
        ok: true,
        ..FrJudgment::default()
    };
    let mut seen = PolyIndex::default();
    for (factor, exponent) in discriminant.factor().factors {
        if exponent % 2 == 0 || factor.is_rational_constant() {
            continue;
        }
        let canonical = factor.canonical_proportional_form();
        if depends_on_any(&canonical, pending)? {
            seen.insert(&mut judgment.carry, canonical);
        } else {
            judgment.kin = 1;
        }
    }
    Ok(judgment)
}

/// One FindRoots carry/discharge step, shared by the gauge scan and the
/// gauge-free LR order search.
pub fn step_fr_judge(
    letters: &[Poly],
    pivot: usize,
    gauge: Option<usize>,
    pending: &[usize],
    all_integration_variables: &[usize],
    state: &mut PathState,
) -> Result<bool> {
    let mut carried = Vec::new();
    let mut carried_index = PolyIndex::default();
    for obligation in &state.carried {
        if depends_on_any(obligation, pending)? || obligation.degree(pivot)? > 0 {
            let canonical = obligation.canonical_proportional_form();
            carried_index.insert(&mut carried, canonical);
        }
    }

    let mut judged = Vec::new();
    let mut judged_index = PolyIndex::default();
    for letter in letters {
        let polynomial = degauge(letter, gauge)?;
        if polynomial.is_rational_constant() {
            continue;
        }
        let canonical = polynomial.canonical_proportional_form();
        judged_index.insert(&mut judged, canonical);
    }
    for obligation in &carried {
        let canonical = obligation.canonical_proportional_form();
        judged_index.insert(&mut judged, canonical);
    }

    for polynomial in judged {
        let judgment = fr_judge(&polynomial, pivot, pending, pending)?;
        if !judgment.ok {
            return Ok(false);
        }
        state.nkin += judgment.kin;
        state.ntq += judgment.terminal;
        for obligation in judgment.carry {
            let canonical = obligation.canonical_proportional_form();
            let (_, inserted) = carried_index.insert(&mut carried, canonical.clone());
            if inserted {
                state.nsq += 1;
                let mut support = 0_usize;
                let mut maximum_degree = 0_i64;
                for &variable in all_integration_variables {
                    let degree = canonical.degree(variable)?;
                    if degree > 0 {
                        support += 1;
                        maximum_degree = maximum_degree.max(degree);
                    }
                }
                if support > 1 || maximum_degree > 2 {
                    state.nonexec = true;
                }
                state.minted_index.insert(&mut state.minted, canonical);
            }
        }
    }
    state.carried = carried;
    Ok(true)
}
