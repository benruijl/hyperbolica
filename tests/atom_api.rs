use hyperbolica::integrator::Boundary;
use hyperbolica::prelude::*;
use hyperbolica::reduce::MzvReductionTable;
use hyperbolica::symbols::mzv_atom;

#[test]
fn atom_api_default_mzv_context_is_embedded_and_basis_only() {
    let x = symbol!("atom_api_standard_mzv_x");
    let standard_options = AtomIntegrationOptions::default();
    let prepared = prepare_atom_with_options(&Atom::one(), &[x], &standard_options).unwrap();
    assert_eq!(
        prepared.context().len(),
        1 + standard_options.mzv_reductions.basis().len()
    );
    assert!(
        prepared
            .context()
            .index_of_indeterminate(mzv_atom(&[2]).as_view())
            .is_some()
    );
    assert!(
        prepared
            .context()
            .index_of_indeterminate(mzv_atom(&[4]).as_view())
            .is_none()
    );

    let empty_options = AtomIntegrationOptions {
        mzv_reductions: MzvReductionTable::default(),
        ..AtomIntegrationOptions::default()
    };
    let empty = prepare_atom_with_options(&Atom::one(), &[x], &empty_options).unwrap();
    assert!(
        empty
            .context()
            .index_of_indeterminate(mzv_atom(&[2]).as_view())
            .is_none()
    );
}

#[test]
fn atom_native_api_regression_suite() {
    let x = symbol!("atom_api_suite_x");
    let mut options = AtomIntegrationOptions {
        check_divergences: true,
        parallel: false,
        ..AtomIntegrationOptions::default()
    };

    // A finite family checks normalization, powers, and endpoint extraction.
    for exponent in 2..=7 {
        let input = Atom::one() / (x + 1).pow(exponent);
        let result = integrate_atom(&input, &[x], &options).unwrap();
        assert_eq!(
            result.to_atom().unwrap(),
            Atom::one() / Atom::num(exponent - 1)
        );
    }

    // Prepared inputs are reusable without repeating Atom-to-ring lowering.
    let y = symbol!("atom_api_suite_y");
    let product = Atom::one() / ((x + 1).pow(2) * (y + 1).pow(2));
    let prepared = prepare_atom(&product, &[x, y]).unwrap();
    assert_eq!(prepared.integration_indices(), &[0, 1]);
    assert_eq!(
        integrate_prepared_atom(&prepared, &options)
            .unwrap()
            .to_atom()
            .unwrap(),
        Atom::one()
    );

    // Never-integrated input parameters are retained in divergence fibration.
    // Upstream records this cross-letter rational as the minimal regression:
    // its x-boundary bins cancel only after projecting them as functions of y.
    let parametric = Atom::one() / ((x + 1) * (x + y));
    let prepared_parametric = prepare_atom(&parametric, &[x]).unwrap();
    assert_eq!(prepared_parametric.integration_indices(), &[0]);
    assert_eq!(prepared_parametric.spectator_indices(), &[1]);
    assert!(
        !integrate_prepared_atom(&prepared_parametric, &options)
            .unwrap()
            .is_zero()
    );

    // A registered function call remains an Atom indeterminate, never a
    // dynamically minted compatibility symbol.
    let algebraic = heads().algebraic_minus.call(1);
    let prepared = prepare_atom(&(Atom::one() / (x + &algebraic)), &[x]).unwrap();
    assert!(
        prepared
            .indeterminates()
            .iter()
            .any(|candidate| candidate == &algebraic)
    );

    // Divergences retain machine-readable boundary data.
    let divergent = Atom::one() / (x + 1);
    let error = integrate_atom(&divergent, &[x], &options).unwrap_err();
    assert!(matches!(
        error,
        AtomIntegrationError::Divergent {
            boundary: Boundary::Infinity,
            ..
        }
    ));

    // Toggling a transport-neutral option does not mutate prepared data.
    options.check_divergences = false;
    assert_eq!(prepared.integration_variables(), &[x]);
}

#[test]
fn atom_api_introduces_structural_quadratic_letters_deterministically() {
    let x = symbol!("atom_api_algebraic_x");
    let input = Atom::one() / (x.pow(2) + 1);
    let options = AtomIntegrationOptions {
        introduce_algebraic_letters: true,
        parallel: true,
        ..AtomIntegrationOptions::default()
    };

    let first = integrate_atom(&input, &[x], &options).unwrap();
    let first_atom = first.to_atom().unwrap();
    assert_eq!(first.algebraic_letters().len(), 1);
    assert_eq!(first.algebraic_letters()[0].idx, 1);
    assert!(first_atom.contains_symbol(heads().algebraic_minus));
    assert!(first_atom.contains_symbol(heads().algebraic_plus));
    assert!(!first.indeterminates().iter().any(|atom| {
        atom.as_var_view().is_some_and(|variable| {
            let symbol = variable.get_symbol();
            let name = symbol.get_name();
            name.contains("Wm_") || name.contains("Wp_")
        })
    }));

    // A fresh request session resets the table and must reproduce both the
    // one-based index and the exact structural Atom.
    let second = integrate_atom(&input, &[x], &options).unwrap();
    assert_eq!(second.algebraic_letters()[0].idx, 1);
    assert_eq!(second.to_atom().unwrap(), first_atom);
}

#[test]
fn atom_api_allows_quadratic_roots_depending_on_a_spectator() {
    let (x, y) = symbol!("atom_api_spectator_root_x", "atom_api_spectator_root_y");
    let input = Atom::one() / (x.pow(2) + y);
    let options = AtomIntegrationOptions {
        introduce_algebraic_letters: true,
        parallel: false,
        ..AtomIntegrationOptions::default()
    };
    let prepared = prepare_atom_with_options(&input, &[x], &options).unwrap();
    assert_eq!(prepared.spectator_indices().len(), 1);

    let output = integrate_prepared_atom(&prepared, &options).unwrap();
    assert_eq!(output.algebraic_letters().len(), 1);
    assert!(
        output.algebraic_letters()[0]
            .polynomial
            .used_variable_indices()
            .contains(&prepared.spectator_indices()[0])
    );
}

#[test]
fn atom_api_rejects_quadratic_roots_depending_on_a_later_variable() {
    let (x, y) = symbol!("atom_api_forbidden_x", "atom_api_forbidden_y");
    let input = Atom::one() / ((x.pow(2) - y) * (y + 1).pow(2));
    let options = AtomIntegrationOptions {
        introduce_algebraic_letters: true,
        parallel: false,
        ..AtomIntegrationOptions::default()
    };
    let error = integrate_atom(&input, &[x, y], &options).unwrap_err();
    assert!(error.to_string().contains("remaining integration variable"));
}
