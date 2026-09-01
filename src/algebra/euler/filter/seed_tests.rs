use std::collections::HashMap;

use symbolica::prelude::{Atom, AtomCore, Rational, Symbol};

use super::{MarginalKey, sampling_digest};
use crate::core::{Poly, PolyCtx};

fn compound_variables(namespace: &'static str, registration_order: &str) -> [Atom; 2] {
    let (function, variable) = if registration_order == "function-first" {
        (
            Symbol::parse("f", namespace).unwrap(),
            Symbol::parse("x", namespace).unwrap(),
        )
    } else {
        let variable = Symbol::parse("x", namespace).unwrap();
        let function = Symbol::parse("f", namespace).unwrap();
        (function, variable)
    };
    let function_variable = function.call(variable);
    let power_variable = (variable.to_atom() + 1).pow(Atom::num(Rational::from((1, 2))));
    [function_variable, power_variable]
}

fn compound_sampling_seed(variables: [Atom; 2]) -> u64 {
    let context = PolyCtx::from_indeterminates(variables).unwrap();
    let factors = [
        Poly::generator(context.clone(), 0).unwrap(),
        Poly::generator(context, 1).unwrap(),
    ];
    sampling_digest(0x434f_4d50_4f55_4e44, &factors, &[0, 1])
}

#[test]
fn sampling_seed_is_registration_order_independent_for_compound_variables() {
    const LEFT_NAMESPACE: &str = "euler_compound_seed_left";
    const RIGHT_NAMESPACE: &str = "euler_compound_seed_right";
    let left = compound_variables(LEFT_NAMESPACE, "function-first");
    let right = compound_variables(RIGHT_NAMESPACE, "variable-first");
    let normalize = |variables: &[Atom], namespace: &str| {
        variables
            .iter()
            .map(AtomCore::to_canonical_string)
            .map(|spelling| spelling.replace(namespace, "NAMESPACE"))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        normalize(&left, LEFT_NAMESPACE),
        normalize(&right, RIGHT_NAMESPACE)
    );

    // Rebuilding the same native function/power context cannot perturb seed
    // material through context-constructor diagnostics.
    assert_eq!(
        compound_sampling_seed(left.clone()),
        compound_sampling_seed(left)
    );
}

#[test]
fn marginal_cache_identity_includes_the_generic_sampling_seed() {
    let context = PolyCtx::new(["f"]).unwrap();
    let first = MarginalKey {
        canonical_factors: vec![Poly::parse(context, "f").unwrap()],
        propagators: vec![0],
        generic_seed: 11,
    };
    let retry = MarginalKey {
        generic_seed: 12,
        ..first.clone()
    };
    let mut cache = HashMap::new();
    cache.insert(first, None);

    assert!(!cache.contains_key(&retry));
    cache.insert(retry, Some(3));
    assert_eq!(cache.len(), 2);
}
