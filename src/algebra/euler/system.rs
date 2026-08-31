//! Pure-Symbolica finite-field Euler-characteristic system driver.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use symbolica::prelude::*;

use crate::core::Poly;

use super::random::DeterministicRng;
#[cfg(test)]
use super::staircase::chi_staircase_count;
use super::staircase::{ChiCount, ChiStatus};

mod field;
mod primes;
mod sector;

#[cfg(test)]
use field::finite_field_image;
use field::{choose_constraint_variable, constraint_root, primitive_integer, validate_indices};
use primes::chi_primes;
#[cfg(test)]
use primes::is_prime;
use sector::count_sector;

type IntegerPoly = MultivariatePolynomial<IntegerRing, u16, LexOrder>;
type FieldPoly = MultivariatePolynomial<Zp, u16, LexOrder>;
type GrevFieldPoly = MultivariatePolynomial<Zp, u16, GrevLexOrder>;

const MAX_SECTOR_VARIABLES: usize = 20;
const RABINOWITSCH_VARIABLE: PolyVariable = PolyVariable::Temporary(usize::MAX);

static SYSTEM_COUNT: AtomicU64 = AtomicU64::new(0);
static SYSTEM_BUILD_NANOS: AtomicU64 = AtomicU64::new(0);
static F4_NANOS: AtomicU64 = AtomicU64::new(0);

/// Coarse process-wide timings for separating ideal construction from F4.
///
/// They are diagnostics, not part of the mathematical result. Counters are
/// atomics so concurrent LR requests cannot race or corrupt accounting.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChiSystemTimings {
    pub systems: u64,
    pub construction: Duration,
    pub f4: Duration,
}

pub fn chi_system_timings() -> ChiSystemTimings {
    ChiSystemTimings {
        systems: SYSTEM_COUNT.load(Ordering::Relaxed),
        construction: Duration::from_nanos(SYSTEM_BUILD_NANOS.load(Ordering::Relaxed)),
        f4: Duration::from_nanos(F4_NANOS.load(Ordering::Relaxed)),
    }
}

pub fn reset_chi_system_timings() {
    SYSTEM_COUNT.store(0, Ordering::Relaxed);
    SYSTEM_BUILD_NANOS.store(0, Ordering::Relaxed);
    F4_NANOS.store(0, Ordering::Relaxed);
}

fn record_nanos(counter: &AtomicU64, duration: Duration) {
    counter.fetch_add(
        u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
}

/// Sum the cleared-dlog quotient dimensions over all propagator sectors.
///
/// Inputs stay in the caller's Symbolica-backed polynomial context.  Factors
/// are converted once to primitive integer associates; all specializations,
/// derivatives, products, finite-field factorization, and GrevLex Groebner
/// bases then use Symbolica's public typed APIs.
pub fn chi_count_sectors(
    factors: &[Poly],
    exponents: &[i64],
    propagator_variables: &[usize],
    parameter_variables: &[usize],
    constraint: Option<&Poly>,
    seed: u64,
) -> ChiCount {
    if !validate_indices(
        factors,
        exponents,
        propagator_variables,
        parameter_variables,
        constraint,
    ) {
        return ChiCount::failed();
    }
    if constraint.is_some_and(Poly::is_zero) {
        return chi_count_sectors(
            factors,
            exponents,
            propagator_variables,
            parameter_variables,
            None,
            seed,
        );
    }

    let integer_factors = factors.iter().map(primitive_integer).collect::<Vec<_>>();
    let integer_constraint = constraint.map(primitive_integer);
    let constraint_variable = integer_constraint.as_ref().and_then(|polynomial| {
        choose_constraint_variable(polynomial, propagator_variables, parameter_variables)
    });
    if integer_constraint.is_some() && constraint_variable.is_none() {
        return ChiCount::failed();
    }

    let variable_count = factors[0].ctx().len();
    let mut rng = DeterministicRng::new(seed);
    for &prime in chi_primes() {
        let field = Zp::new(prime);
        let mut residues = vec![0_u32; variable_count];
        for variable in parameter_variables {
            residues[*variable] = rng.inclusive(2, prime - 2);
        }
        if let (Some(polynomial), Some(variable)) =
            (integer_constraint.as_ref(), constraint_variable)
        {
            let Some(root) =
                constraint_root(polynomial, variable, parameter_variables, &residues, &field)
            else {
                continue;
            };
            residues[variable] = root;
        }

        let mut total = 0_u64;
        let mut positive_dimensional = false;
        let mut failed = false;
        for mask in 0..(1_u64 << propagator_variables.len()) {
            match count_sector(
                &integer_factors,
                exponents,
                propagator_variables,
                parameter_variables,
                &residues,
                mask,
                &field,
            ) {
                ChiCount {
                    status: ChiStatus::Finite,
                    count,
                } => {
                    let Some(sum) = total.checked_add(count) else {
                        return ChiCount::failed();
                    };
                    total = sum;
                }
                ChiCount {
                    status: ChiStatus::PositiveDim,
                    ..
                } => positive_dimensional = true,
                ChiCount {
                    status: ChiStatus::Failed,
                    ..
                } => {
                    failed = true;
                    break;
                }
            }
        }
        if failed {
            continue;
        }
        return if positive_dimensional {
            ChiCount::positive_dimensional()
        } else {
            ChiCount::finite(total)
        };
    }
    ChiCount::failed()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::core::PolyCtx;

    use super::*;

    fn parse(context: &Arc<PolyCtx>, expression: &str) -> Poly {
        Poly::parse(context.clone(), expression).unwrap()
    }

    #[test]
    fn generated_prime_table_matches_the_contract() {
        let primes = chi_primes();
        assert_eq!(primes.len(), 32);
        assert_eq!(primes[0], 2_147_483_647);
        assert!(primes.windows(2).all(|pair| pair[0] > pair[1]));
        assert!(primes.iter().all(|prime| is_prime(*prime)));
    }

    #[test]
    fn tiny_native_f4_roundtrip_has_four_standard_monomials() {
        let context = PolyCtx::new(["x", "y"]).unwrap();
        let field = Zp::new(65_521);
        let polys = [parse(&context, "x^2+y"), parse(&context, "y^2-3")];
        let system = polys
            .iter()
            .map(primitive_integer)
            .map(|polynomial| finite_field_image(&polynomial, &field).reorder::<GrevLexOrder>())
            .collect::<Vec<_>>();
        let basis = GroebnerBasis::<Zp, u16, GrevLexOrder>::new(&system, false);
        let leading = basis
            .system
            .iter()
            .map(|polynomial| polynomial.max_exp().to_vec())
            .collect::<Vec<_>>();
        assert_eq!(chi_staircase_count(&leading, 2), ChiCount::finite(4));
    }

    #[test]
    fn invalid_inputs_fail_conservatively_without_entering_f4() {
        let context = PolyCtx::new(["x", "s"]).unwrap();
        let factor = parse(&context, "x+s");
        assert_eq!(
            chi_count_sectors(&[], &[], &[0], &[1], None, 1).status,
            ChiStatus::Failed
        );
        assert_eq!(
            chi_count_sectors(std::slice::from_ref(&factor), &[], &[0], &[1], None, 1).status,
            ChiStatus::Failed
        );
        assert_eq!(
            chi_count_sectors(std::slice::from_ref(&factor), &[7], &[0], &[0], None, 1).status,
            ChiStatus::Failed
        );
        assert_eq!(
            chi_count_sectors(
                std::slice::from_ref(&factor),
                &[7],
                &[0],
                &[1],
                Some(&parse(&context, "x+s")),
                1
            )
            .status,
            ChiStatus::Failed
        );
    }

    #[test]
    fn box_constraint_oracles() {
        let context = PolyCtx::new(["x1", "x2", "x3", "x4", "s", "t"]).unwrap();
        let box_factor = parse(&context, "x1+x2+x3+x4+s*x1*x3+t*x2*x4");
        let generic = chi_count_sectors(
            std::slice::from_ref(&box_factor),
            &[101],
            &[0, 1, 2, 3],
            &[4, 5],
            None,
            20_260_604,
        );
        assert_eq!(generic, ChiCount::finite(3));
        for (constraint, seed, expected) in [
            ("s", 20_260_605, 1),
            ("s+t", 20_260_606, 2),
            ("s+7*t", 20_260_607, 3),
        ] {
            assert_eq!(
                chi_count_sectors(
                    std::slice::from_ref(&box_factor),
                    &[101],
                    &[0, 1, 2, 3],
                    &[4, 5],
                    Some(&parse(&context, constraint)),
                    seed,
                ),
                ChiCount::finite(expected)
            );
        }
    }

    #[test]
    fn par_generic_zero_nonlinear_constraint_oracle() {
        let context = PolyCtx::new(["x1", "x2", "x3", "x4", "m1", "m2", "m3", "m4", "s"]).unwrap();
        let factor = parse(
            &context,
            "x1*x3+x1*x4+x2*x3+x2*x4+x3*x4\
             -m1*x1^2*x3-m1*x1^2*x4+(-m1-m2+s)*x1*x2*x3\
             +(-m1-m2+s)*x1*x2*x4-m3*x1*x3^2\
             +(-m1-m3-m4)*x1*x3*x4-m4*x1*x4^2-m2*x2^2*x3\
             -m2*x2^2*x4-m3*x2*x3^2+(-m2-m3-m4)*x2*x3*x4\
             -m4*x2*x4^2-m3*x3^2*x4-m4*x3*x4^2",
        );
        let propagators = [0, 1, 2, 3];
        let parameters = [4, 5, 6, 7, 8];
        assert_eq!(
            chi_count_sectors(
                std::slice::from_ref(&factor),
                &[757],
                &propagators,
                &parameters,
                None,
                20_260_608,
            ),
            ChiCount::finite(13)
        );
        assert_eq!(
            chi_count_sectors(
                std::slice::from_ref(&factor),
                &[757],
                &propagators,
                &parameters,
                Some(&parse(&context, "m1^2-2*m1*m3-2*m1*m4+m3^2-2*m3*m4+m4^2",)),
                20_260_609,
            ),
            ChiCount::finite(12)
        );
        assert_eq!(
            chi_count_sectors(
                &[factor],
                &[757],
                &propagators,
                &parameters,
                Some(&parse(&context, "m1+7*m2-3*s")),
                20_260_610,
            ),
            ChiCount::finite(13)
        );
    }

    #[test]
    fn exponent_values_do_not_change_the_box_count() {
        let context = PolyCtx::new(["x1", "x2", "x3", "x4", "s", "t"]).unwrap();
        let factor = parse(&context, "x1+x2+x3+x4+s*x1*x3+t*x2*x4");
        let low = chi_count_sectors(
            std::slice::from_ref(&factor),
            &[7],
            &[0, 1, 2, 3],
            &[4, 5],
            None,
            20_260_611,
        );
        let high = chi_count_sectors(
            std::slice::from_ref(&factor),
            &[104_729],
            &[0, 1, 2, 3],
            &[4, 5],
            None,
            20_260_612,
        );
        assert_eq!(low, ChiCount::finite(3));
        assert_eq!(high, low);
    }

    #[test]
    fn split_pair_exponent_independence() {
        let context = PolyCtx::new(["x1", "x2", "x3", "s", "t"]).unwrap();
        let factors = [
            parse(&context, "x1+x2+x3+1"),
            parse(&context, "s*x1*x3+t*x2"),
        ];
        let first = chi_count_sectors(&factors, &[103, 211], &[0, 1, 2], &[3, 4], None, 20_260_613);
        let second = chi_count_sectors(
            &factors,
            &[3001, 65_537],
            &[0, 1, 2],
            &[3, 4],
            None,
            20_260_614,
        );
        assert_eq!(first, ChiCount::finite(3));
        assert_eq!(second, first);
    }

    #[test]
    fn repeated_seed_is_deterministic() {
        let context = PolyCtx::new(["x", "s"]).unwrap();
        let factor = parse(&context, "1+x+s*x");
        let first = chi_count_sectors(std::slice::from_ref(&factor), &[127], &[0], &[1], None, 99);
        let second = chi_count_sectors(&[factor], &[127], &[0], &[1], None, 99);
        assert_eq!(first, second);
    }
}
