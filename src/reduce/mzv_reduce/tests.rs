use symbolica::prelude::{AtomCore, symbol};

use super::*;
use crate::core::PolyCtx;
use crate::symbols::mzv_atom;

fn table() -> MzvReductionTable {
    MzvReductionTable::from_parts(
        vec![
            MzvReductionRule {
                lhs: "mzv_4".into(),
                rhs: "2/5*mzv_2^2".into(),
            },
            MzvReductionRule {
                lhs: "mzv_6".into(),
                rhs: "8/35*mzv_2^3".into(),
            },
        ],
        vec!["Log2".into(), "mzv_2".into(), "mzv_3".into()],
    )
}

#[test]
fn rational_substitution_handles_denominators_and_precedence() {
    let ctx = PolyCtx::new(["x", "y"]).unwrap();
    let rational = Rat::parse(ctx.clone(), "(x^2+y)/(1-x)").unwrap();
    let replacement = Rat::parse(ctx.clone(), "y/(1+y)").unwrap();
    let actual = substitute_var_rat(&rational, 0, &replacement).unwrap();
    let expected = Rat::parse(ctx, "((y/(1+y))^2+y)/(1-y/(1+y))").unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn fixed_point_reduction_is_exact_and_skips_absent_rules() {
    let z = symbol!("fixed_point_mzv_z");
    let ctx = PolyCtx::from_indeterminates(build_mzv_atom_list(&table(), [z.to_atom()]).unwrap())
        .unwrap();
    let input_atom: Atom = 3 * mzv_atom(&[4]) / (1 + z);
    let input = Rat::from_atom(ctx.clone(), input_atom.as_view()).unwrap();
    let actual = apply_mzv_reductions(&table(), &input).unwrap();
    let expected: Atom = (6 * mzv_atom(&[2]).pow(2)) / (5 * (1 + z));
    assert_eq!(actual, Rat::from_atom(ctx, expected.as_view()).unwrap());
    assert!(!input.compatibility_views_initialized());
    assert!(!actual.compatibility_views_initialized());
}

#[test]
fn variable_list_has_stable_source_order() {
    assert_eq!(
        build_mzv_var_list(&table(), ["x", "Log2"]),
        ["x", "Log2", "mzv_2", "mzv_3", "mzv_4", "mzv_6"]
    );
}

#[test]
fn native_mzv_atoms_use_registered_parameterized_heads() {
    assert_eq!(indices_from_mzv_name("mzv_m2_3"), Some(vec![-2, 3]));
    assert_eq!(indices_from_mzv_name("mzv_"), None);

    let atoms = build_mzv_atom_list(&table(), Vec::new()).unwrap();
    assert!(atoms.contains(&mzv_atom(&[2])));
    assert!(atoms.contains(&mzv_atom(&[4])));
    assert!(atoms.contains(&log_two_atom()));
    assert!(!atoms.iter().any(|atom| {
        atom.as_var_view()
            .is_some_and(|variable| variable.get_symbol().get_name().contains("mzv_"))
    }));

    let expression = mzv_expression_atom("2/5*mzv_2^2+Log2*mzv_m2_3").unwrap();
    assert!(expression.contains_symbol(crate::symbols::heads().mzv));
    assert!(expression.contains_symbol(crate::symbols::heads().log_two));
}

#[test]
fn embedded_standard_table_is_shared_and_basis_only_is_narrow() {
    let first = standard_mzv_reductions();
    let second = standard_mzv_reductions();
    assert!(first.is_embedded_standard());
    assert!(second.is_embedded_standard());
    assert_eq!(first.reductions().len(), EMBEDDED_MZV_REDUCTION_COUNT);
    assert_eq!(first.basis().len(), EMBEDDED_MZV_BASIS_COUNT);
    assert!(first.reductions().len() > first.basis().len());

    let basis = build_mzv_basis_atom_list(&first, Vec::new()).unwrap();
    let wide = build_mzv_atom_list(&first, Vec::new()).unwrap();
    assert_eq!(basis.len(), first.basis().len());
    assert!(wide.len() > basis.len());

    let user = vec!["x".to_owned()];
    let no_mzv = build_narrow_var_list(&first, &user, "1/(1+x)");
    assert_eq!(no_mzv.len(), first.basis().len() + 1);
    assert!(no_mzv.contains(&"x".to_owned()));
    assert!(!no_mzv.contains(&first.reductions()[0].lhs));
}

#[test]
fn semantic_standard_copies_recover_constant_time_identity() {
    let standard = standard_mzv_reductions();
    let from_parts =
        MzvReductionTable::from_parts(standard.reductions().to_vec(), standard.basis().to_vec());
    assert!(from_parts.is_embedded_standard());

    let encoded = serde_json::to_vec(&standard).unwrap();
    let decoded: MzvReductionTable = serde_json::from_slice(&encoded).unwrap();
    assert!(decoded.is_embedded_standard());

    let mut through_setters = MzvReductionTable::default();
    through_setters.set_reductions(standard.reductions().to_vec());
    assert!(!through_setters.is_embedded_standard());
    through_setters.set_basis(standard.basis().to_vec());
    assert!(through_setters.is_embedded_standard());

    // No-op setters retain the canonical allocation, without Arc cloning.
    through_setters.set_reductions(standard.reductions().to_vec());
    through_setters.set_basis(standard.basis().to_vec());
    assert!(through_setters.is_embedded_standard());
}

#[test]
fn reductions_operate_on_registered_function_indeterminates() {
    let z = symbol!("native_mzv_z");
    let indeterminates = build_mzv_atom_list(&table(), [z.to_atom()]).unwrap();
    let ctx = PolyCtx::from_indeterminates(indeterminates).unwrap();
    let input_atom: Atom = 3 * mzv_atom(&[4]) / (1 + z);
    let input = Rat::from_atom(ctx.clone(), input_atom.as_view()).unwrap();
    let actual = apply_mzv_reductions(&table(), &input).unwrap();
    let expected_atom: Atom = (6 * mzv_atom(&[2]).pow(2)) / (5 * (1 + z));
    let expected = Rat::from_atom(ctx, expected_atom.as_view()).unwrap();
    assert_eq!(actual, expected);
}
