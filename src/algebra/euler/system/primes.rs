//! Deterministic large-prime selection for finite-field probes.

use std::sync::OnceLock;

pub(super) fn modular_power(mut base: u64, mut exponent: u64, modulus: u64) -> u64 {
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

pub(super) fn is_prime(value: u32) -> bool {
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

pub(super) fn chi_primes() -> &'static [u32] {
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
