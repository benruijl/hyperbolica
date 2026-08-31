use std::collections::{HashSet, VecDeque};

/// Outcome of an Euler-characteristic sector count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChiStatus {
    /// The quotient has finite vector-space dimension.
    Finite,
    /// The leading ideal leaves an infinite standard-monomial staircase.
    PositiveDim,
    /// Input validation or a native finite-field/Groebner operation failed.
    Failed,
}

/// A finite quotient dimension, or a conservative non-finite status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChiCount {
    pub status: ChiStatus,
    /// Valid only when `status == ChiStatus::Finite`.
    pub count: u64,
}

impl ChiCount {
    pub const fn finite(count: u64) -> Self {
        Self {
            status: ChiStatus::Finite,
            count,
        }
    }

    pub const fn positive_dimensional() -> Self {
        Self {
            status: ChiStatus::PositiveDim,
            count: 0,
        }
    }

    pub const fn failed() -> Self {
        Self {
            status: ChiStatus::Failed,
            count: 0,
        }
    }
}

/// Count standard monomials from the leading exponents of a Groebner basis.
///
/// This is the retained combinatorial `dpFindIrredMonos` staircase walk.  The
/// CAS-facing half obtains `lead_exponents` directly from Symbolica's public
/// `max_exp`; this function deliberately does not solve the ideal or enumerate
/// roots, because quotient dimension includes multiplicity.
pub fn chi_staircase_count(lead_exponents: &[Vec<u16>], variable_count: usize) -> ChiCount {
    if variable_count == 0 {
        return ChiCount::finite(1);
    }
    if lead_exponents.is_empty()
        || lead_exponents
            .iter()
            .any(|exponents| exponents.len() != variable_count)
    {
        return if lead_exponents.is_empty() {
            ChiCount::positive_dimensional()
        } else {
            ChiCount::failed()
        };
    }

    let mut has_origin = false;
    let mut pure_bounds = vec![None::<u16>; variable_count];
    for exponents in lead_exponents {
        let mut support = exponents
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, exponent)| *exponent != 0);
        match (support.next(), support.next()) {
            (None, _) => has_origin = true,
            (Some((variable, exponent)), None) => {
                pure_bounds[variable] =
                    Some(pure_bounds[variable].map_or(exponent, |bound| bound.min(exponent)));
            }
            _ => {}
        }
    }

    if !has_origin && pure_bounds.iter().any(Option::is_none) {
        return ChiCount::positive_dimensional();
    }

    let divides = |leading: &[u16], monomial: &[u16]| {
        leading
            .iter()
            .zip(monomial)
            .all(|(left, right)| right >= left)
    };
    let origin = vec![0_u16; variable_count];
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([origin]);
    let mut count = 0_u64;

    while let Some(monomial) = queue.pop_front() {
        if !seen.insert(monomial.clone()) {
            continue;
        }
        if lead_exponents
            .iter()
            .any(|leading| divides(leading, &monomial))
        {
            continue;
        }
        count = match count.checked_add(1) {
            Some(value) => value,
            None => return ChiCount::failed(),
        };
        for (variable, bound) in pure_bounds.iter().copied().enumerate() {
            let Some(bound) = bound else { continue };
            if monomial[variable] < bound.saturating_sub(1) {
                let mut child = monomial.clone();
                child[variable] += 1;
                queue.push_back(child);
            }
        }
    }

    ChiCount::finite(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_staircase_fixtures() {
        assert_eq!(
            chi_staircase_count(&[vec![2, 0], vec![0, 2]], 2),
            ChiCount::finite(4)
        );
        assert_eq!(
            chi_staircase_count(&[vec![1, 1]], 2).status,
            ChiStatus::PositiveDim
        );
        assert_eq!(chi_staircase_count(&[vec![0, 0]], 2), ChiCount::finite(0));
        assert_eq!(
            chi_staircase_count(&[vec![3, 0], vec![1, 1], vec![0, 2]], 2),
            ChiCount::finite(4)
        );
    }

    #[test]
    fn staircase_corner_cases_are_total() {
        assert_eq!(chi_staircase_count(&[], 0), ChiCount::finite(1));
        assert_eq!(chi_staircase_count(&[], 3).status, ChiStatus::PositiveDim);
        assert_eq!(
            chi_staircase_count(&[vec![1, 0]], 3).status,
            ChiStatus::PositiveDim
        );
        assert_eq!(
            chi_staircase_count(&[vec![0, 0, 0]], 3),
            ChiCount::finite(0)
        );
        assert_eq!(chi_staircase_count(&[vec![1]], 2).status, ChiStatus::Failed);
    }
}
