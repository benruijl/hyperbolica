use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use symbolica::prelude::*;

use crate::core::Poly;

use super::random::DeterministicRng;
use super::staircase::{ChiCount, ChiStatus, chi_staircase_count};

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

fn primitive_integer(polynomial: &Poly) -> IntegerPoly {
    if polynomial.is_zero() {
        return MultivariatePolynomial::new(&Z, None, polynomial.inner().get_vars());
    }
    let denominator_lcm = polynomial
        .inner()
        .coefficients
        .iter()
        .fold(Integer::one(), |lcm, coefficient| {
            lcm.lcm(coefficient.denominator_ref())
        });
    polynomial
        .inner()
        .map_coeff(
            |coefficient| {
                Z.mul(
                    coefficient.numerator_ref(),
                    &Z.quot(&denominator_lcm, coefficient.denominator_ref()),
                )
            },
            Z,
        )
        .make_primitive()
}

fn modular_power(mut base: u64, mut exponent: u64, modulus: u64) -> u64 {
    let mut result = 1_u64;
    base %= modulus;
    while exponent != 0 {
        if exponent & 1 != 0 {
            result = (u128::from(result) * u128::from(base) % u128::from(modulus)) as u64;
        }
        exponent >>= 1;
        if exponent != 0 {
            base = (u128::from(base) * u128::from(base) % u128::from(modulus)) as u64;
        }
    }
    result
}

fn is_prime(value: u32) -> bool {
    if value < 2 {
        return false;
    }
    for small in [2_u32, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
        if value == small {
            return true;
        }
        if value.is_multiple_of(small) {
            return false;
        }
    }
    let mut odd = u64::from(value - 1);
    let shifts = odd.trailing_zeros();
    odd >>= shifts;
    'witness: for base in [2_u64, 3, 5, 7, 11] {
        if base >= u64::from(value) {
            continue;
        }
        let mut residue = modular_power(base, odd, u64::from(value));
        if residue == 1 || residue == u64::from(value - 1) {
            continue;
        }
        for _ in 1..shifts {
            residue = (u128::from(residue) * u128::from(residue) % u128::from(value)) as u64;
            if residue == u64::from(value - 1) {
                continue 'witness;
            }
        }
        return false;
    }
    true
}

fn chi_primes() -> &'static [u32] {
    static PRIMES: OnceLock<Vec<u32>> = OnceLock::new();
    PRIMES.get_or_init(|| {
        let mut primes = Vec::with_capacity(32);
        let mut candidate = i32::MAX as u32;
        while primes.len() < 32 {
            if is_prime(candidate) {
                primes.push(candidate);
            }
            candidate -= 1;
        }
        primes
    })
}

fn validate_indices(
    factors: &[Poly],
    exponents: &[i64],
    propagator_variables: &[usize],
    parameter_variables: &[usize],
    constraint: Option<&Poly>,
) -> bool {
    if factors.is_empty()
        || factors.len() != exponents.len()
        || propagator_variables.len() > MAX_SECTOR_VARIABLES
    {
        return false;
    }
    let Some(first) = factors.first() else {
        return false;
    };
    if factors
        .iter()
        .any(|factor| factor.ctx().vars() != first.ctx().vars())
        || constraint.is_some_and(|value| value.ctx().vars() != first.ctx().vars())
    {
        return false;
    }
    let mut seen = HashSet::with_capacity(propagator_variables.len() + parameter_variables.len());
    if propagator_variables
        .iter()
        .chain(parameter_variables)
        .any(|variable| *variable >= first.ctx().len() || !seen.insert(*variable))
    {
        return false;
    }
    factors.iter().chain(constraint).all(|polynomial| {
        polynomial
            .used_variable_indices()
            .iter()
            .all(|variable| seen.contains(variable))
    })
}

fn finite_field_image(polynomial: &IntegerPoly, field: &Zp) -> FieldPoly {
    polynomial.map_coeff(|coefficient| field.nth(coefficient.clone()), field.clone())
}

fn choose_constraint_variable(
    constraint: &IntegerPoly,
    propagator_variables: &[usize],
    parameter_variables: &[usize],
) -> Option<usize> {
    if propagator_variables
        .iter()
        .any(|variable| constraint.degree(*variable) != 0)
    {
        return None;
    }
    parameter_variables
        .iter()
        .copied()
        .filter(|variable| constraint.degree(*variable) != 0)
        .min_by_key(|variable| constraint.degree(*variable))
}

fn constraint_root(
    constraint: &IntegerPoly,
    constraint_variable: usize,
    parameter_variables: &[usize],
    residues: &[u32],
    field: &Zp,
) -> Option<u32> {
    let mut univariate = finite_field_image(constraint, field);
    for variable in parameter_variables {
        if *variable != constraint_variable {
            univariate = univariate.replace(*variable, &field.to_element(residues[*variable]));
        }
    }
    if univariate.degree(constraint_variable) == 0 {
        return None;
    }

    let factors = catch_unwind(AssertUnwindSafe(|| univariate.factor())).ok()?;
    factors
        .into_iter()
        .filter_map(|(factor, _)| {
            if factor.degree(constraint_variable) != 1
                || (0..factor.nvars())
                    .any(|variable| variable != constraint_variable && factor.degree(variable) != 0)
            {
                return None;
            }
            let root = field.neg(&field.div(&factor.get_constant(), &factor.lcoeff()));
            Some(field.from_element(&root))
        })
        .min()
}

fn specialize_and_project(
    polynomial: &IntegerPoly,
    active_propagators: &[usize],
    inactive_propagators: &[usize],
    parameter_variables: &[usize],
    residues: &[u32],
    field: &Zp,
) -> FieldPoly {
    let mut specialized = finite_field_image(polynomial, field);
    let zero = field.zero();
    for variable in inactive_propagators {
        specialized = specialized.replace(*variable, &zero);
    }
    for variable in parameter_variables {
        specialized = specialized.replace(*variable, &field.to_element(residues[*variable]));
    }

    let mut variables = Vec::with_capacity(active_propagators.len() + 1);
    variables.push(RABINOWITSCH_VARIABLE);
    variables.extend(
        active_propagators
            .iter()
            .map(|variable| polynomial.get_vars_ref()[*variable].clone()),
    );
    let mut projected = FieldPoly::new(field, Some(specialized.nterms()), Arc::new(variables));
    let mut exponents = vec![0_u16; active_propagators.len() + 1];
    for monomial in &specialized {
        for (target, source) in active_propagators.iter().copied().enumerate() {
            exponents[target + 1] = monomial.exponents[source];
        }
        projected.append_monomial(*monomial.coefficient, &exponents);
    }
    projected
}

fn cleared_dlog_system(factors: &[FieldPoly], exponents: &[i64]) -> Vec<GrevFieldPoly> {
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

fn count_sector(
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
