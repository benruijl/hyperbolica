//! Cleared logarithmic-derivative systems and one-sector F4 count.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::Ordering;
use std::time::Instant;

use symbolica::prelude::*;

use super::super::staircase::{ChiCount, chi_staircase_count};
use super::field::specialize_and_project;
use super::{
    F4_NANOS, FieldPoly, GrevFieldPoly, IntegerPoly, RABINOWITSCH_VARIABLE, SYSTEM_BUILD_NANOS,
    SYSTEM_COUNT, record_nanos,
};

pub(super) fn cleared_dlog_system(factors: &[FieldPoly], exponents: &[i64]) -> Vec<GrevFieldPoly> {
    debug_assert!(!factors.is_empty());
    let count = factors.len();
    let template = &factors[0];
    let mut prefix = Vec::with_capacity(count + 1);
    prefix.push(template.one());
    for factor in factors {
        prefix.push(prefix.last().expect("prefix seed").clone() * factor);
    }
    let mut suffix = vec![template.one(); count + 1];
    for index in (0..count).rev() {
        suffix[index] = &suffix[index + 1] * &factors[index];
    }

    let mut system = Vec::with_capacity(template.nvars());
    for variable in 1..template.nvars() {
        let mut numerator = template.zero();
        for index in 0..count {
            let derivative = factors[index].derivative(variable);
            if derivative.is_zero() || exponents[index] == 0 {
                continue;
            }
            let product_without = &prefix[index] * &suffix[index + 1];
            let coefficient = template.ring().nth(exponents[index].into());
            let term = (derivative * &product_without).mul_coeff(coefficient);
            numerator = &numerator + &term;
        }
        if !numerator.is_zero() {
            system.push(numerator.reorder::<GrevLexOrder>());
        }
    }

    let rabinowitsch = template
        .variable(&RABINOWITSCH_VARIABLE)
        .expect("the internal Rabinowitsch variable is present");
    let product = &rabinowitsch * &prefix[count];
    let rabinowitsch_equation = &template.one() - &product;
    system.push(rabinowitsch_equation.reorder::<GrevLexOrder>());
    system
}

pub(super) fn count_sector(
    factors: &[IntegerPoly],
    exponents: &[i64],
    propagator_variables: &[usize],
    parameter_variables: &[usize],
    residues: &[u32],
    mask: u64,
    field: &Zp,
) -> ChiCount {
    let active = propagator_variables
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(bit, variable)| (mask & (1_u64 << bit) != 0).then_some(variable))
        .collect::<Vec<_>>();
    let inactive = propagator_variables
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(bit, variable)| (mask & (1_u64 << bit) == 0).then_some(variable))
        .collect::<Vec<_>>();

    let construction_started = Instant::now();
    let specialized = factors
        .iter()
        .map(|factor| {
            specialize_and_project(
                factor,
                &active,
                &inactive,
                parameter_variables,
                residues,
                field,
            )
        })
        .collect::<Vec<_>>();
    if specialized.iter().any(MultivariatePolynomial::is_zero) {
        record_nanos(&SYSTEM_BUILD_NANOS, construction_started.elapsed());
        return ChiCount::finite(0);
    }
    let system = cleared_dlog_system(&specialized, exponents);
    record_nanos(&SYSTEM_BUILD_NANOS, construction_started.elapsed());
    SYSTEM_COUNT.fetch_add(1, Ordering::Relaxed);

    let f4_started = Instant::now();
    let basis = catch_unwind(AssertUnwindSafe(|| {
        GroebnerBasis::<Zp, u16, GrevLexOrder>::new(&system, false)
    }));
    record_nanos(&F4_NANOS, f4_started.elapsed());
    let Ok(basis) = basis else {
        return ChiCount::failed();
    };
    let leading_exponents = basis
        .system
        .iter()
        .filter(|polynomial| !polynomial.is_zero())
        .map(|polynomial| polynomial.max_exp().to_vec())
        .collect::<Vec<_>>();
    chi_staircase_count(&leading_exponents, active.len() + 1)
}
