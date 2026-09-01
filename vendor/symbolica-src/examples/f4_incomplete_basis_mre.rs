//! Regression for Symbolica F4 returning an incomplete basis before the
//! pre-matrix simplification-basis snapshot fix.
//!
//! Run with:
//! `cargo run --example f4_incomplete_basis_mre`

use std::sync::Arc;

use symbolica::{
    atom::AtomCore,
    domains::finite_field::Zp,
    parse,
    poly::{
        GrevLexOrder, PolyVariable, groebner::GroebnerBasis, polynomial::MultivariatePolynomial,
    },
    symbol,
};

fn main() {
    let field = Zp::new(65_521);
    let variables = Arc::new(vec![
        PolyVariable::from(symbol!("x")),
        PolyVariable::from(symbol!("y")),
        PolyVariable::from(symbol!("z")),
    ]);
    let ideal: Vec<MultivariatePolynomial<_, u16, GrevLexOrder>> =
        ["206*x*z+942*y", "422*x^2+422*x*y"]
            .iter()
            .map(|polynomial| {
                parse!(polynomial)
                    .to_polynomial::<_, u16>(&field, Some(variables.clone()))
                    .reorder::<GrevLexOrder>()
            })
            .collect();

    let basis = GroebnerBasis::new(&ideal, false);

    // Exercise raw F4: the defensive Buchberger completion must not hide a
    // native algorithm regression here.
    assert!(
        GroebnerBasis::is_groebner_basis(&basis.system),
        "F4 returned an incomplete basis"
    );
    assert!(
        ideal
            .iter()
            .all(|polynomial| polynomial.reduce(&basis.system).is_zero()),
        "F4 changed the generated ideal"
    );
    assert!(
        basis
            .system
            .iter()
            .any(|polynomial| polynomial.max_exp() == [0, 2, 1]),
        "F4 omitted the y^2*z reducer"
    );
}
