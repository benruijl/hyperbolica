//! Serialized algebraic-letter pairs and their Vieta reductions.
//!
//! A pair `Wm_i`, `Wp_i` denotes the two roots of a quadratic in one
//! selected variable. The registry keeps the original quadratic and its
//! Vieta data while rational expressions continue to use ordinary variables
//! in their [`crate::core::PolyCtx`].

#[cfg(test)]
use crate::core::PolyCtx;
use crate::core::{Poly, Rat};
use crate::symbols::{AlgebraicAtoms, algebraic_atoms};

mod atoms;
mod registry;
mod substitution;
mod vieta;

pub use atoms::{build_algebraic_letter_atom_list, build_full_atom_list};
pub(crate) use registry::join_algebraic_letter_session;
pub use registry::{
    AlgebraicLetterSession, AlgebraicLetterTable, algebraic_letters_allocate,
    algebraic_letters_clear, algebraic_letters_show, algebraic_letters_size,
    begin_algebraic_letter_session,
};
pub use substitution::{back_substitute, combine_wm_wp_ratios};
pub use vieta::simplify_with_vieta;

pub const DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE: usize = 16;

/// Metadata associated with one one-based `Wm_i`/`Wp_i` pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlgebraicLetterEntry {
    pub idx: usize,
    pub polynomial: Poly,
    pub var_idx: usize,
    pub lc: Rat,
    pub sum_value: Rat,
    pub product_value: Rat,
    pub discriminant: Poly,
}

impl AlgebraicLetterEntry {
    /// Registered function indeterminates for this root pair.
    pub fn atoms(&self) -> AlgebraicAtoms {
        algebraic_atoms(
            u32::try_from(self.idx).expect("allocated algebraic-letter index must fit in u32"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use symbolica::prelude::{AtomCore, Symbol, symbol};

    use crate::symbols::{SYMBOL_NAMESPACE, heads, legacy};

    fn context() -> std::sync::Arc<PolyCtx> {
        let x = Symbol::parse("x", SYMBOL_NAMESPACE).unwrap();
        PolyCtx::from_indeterminates(build_algebraic_letter_atom_list(
            [x.to_atom()],
            DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE,
        ))
        .unwrap()
    }

    fn rat(ctx: &std::sync::Arc<PolyCtx>, expression: &str) -> Rat {
        let atom = legacy::parse_expression(expression).unwrap();
        Rat::from_atom(ctx.clone(), atom.as_view()).unwrap()
    }

    fn poly(ctx: &std::sync::Arc<PolyCtx>, expression: &str) -> Poly {
        let atom = legacy::parse_expression(expression).unwrap();
        Poly::from_atom(ctx.clone(), atom.as_view()).unwrap()
    }

    fn allocate_x_squared_minus_five(ctx: &std::sync::Arc<PolyCtx>) -> AlgebraicLetterEntry {
        let polynomial = poly(ctx, "x^2-5");
        let idx = algebraic_letters_allocate(&polynomial, 0).unwrap();
        AlgebraicLetterTable::global().at(idx).unwrap()
    }

    #[test]
    fn atom_pool_and_allocation_are_deterministic_and_deduplicated() {
        let _session = begin_algebraic_letter_session().unwrap();
        let ctx = context();
        assert!(
            ctx.variable_atom(1)
                .unwrap()
                .as_fun_view()
                .is_some_and(|function| function.get_symbol() == heads().algebraic_minus)
        );
        assert!(!(0..ctx.len()).any(|index| {
            ctx.variable_atom(index)
                .unwrap()
                .as_var_view()
                .is_some_and(|variable| {
                    variable
                        .get_symbol()
                        .get_name()
                        .starts_with("hyperbolica::Wm_")
                })
        }));
        let entry = allocate_x_squared_minus_five(&ctx);
        assert_eq!(entry.idx, 1);
        assert_eq!(entry.lc, rat(&ctx, "1"));
        assert_eq!(entry.sum_value, rat(&ctx, "0"));
        assert_eq!(entry.product_value, rat(&ctx, "-5"));
        assert_eq!(entry.discriminant, poly(&ctx, "20"));
        assert_eq!(
            algebraic_letters_allocate(&poly(&ctx, "x^2-5"), 0).unwrap(),
            1
        );
        assert_eq!(algebraic_letters_size().unwrap(), 1);

        let error =
            algebraic_letters_allocate(&poly(entry.polynomial.ctx(), "x+1"), 0).unwrap_err();
        assert!(error.to_string().contains("degree 2"));
    }

    #[test]
    fn ratio_combination_and_back_substitution_match_pair_definitions() {
        let _session = begin_algebraic_letter_session().unwrap();
        let ctx = context();
        allocate_x_squared_minus_five(&ctx);

        let ratio = rat(&ctx, "Wm_1/Wp_1");
        assert_eq!(
            combine_wm_wp_ratios(&ratio).unwrap(),
            rat(&ctx, "WmOverWp_1")
        );
        let difference = rat(&ctx, "Wm_1-Wp_1");
        assert_eq!(
            back_substitute(&difference).unwrap(),
            rat(&ctx, "-sqrt_disc_1")
        );
    }

    #[test]
    fn vieta_reduces_powers_and_products_but_preserves_atom_denominators() {
        let _session = begin_algebraic_letter_session().unwrap();
        let ctx = context();
        allocate_x_squared_minus_five(&ctx);

        assert_eq!(
            simplify_with_vieta(&rat(&ctx, "Wm_1*Wp_1")).unwrap(),
            rat(&ctx, "-5")
        );
        assert_eq!(
            simplify_with_vieta(&rat(&ctx, "Wm_1^2-5")).unwrap(),
            Rat::zero(ctx.clone())
        );

        let denominator_has_atom = rat(&ctx, "1/(1+Wm_1)");
        assert_eq!(
            simplify_with_vieta(&denominator_has_atom).unwrap(),
            denominator_has_atom
        );
    }

    #[test]
    fn ordinary_function_indeterminates_survive_the_native_pool() {
        let f = symbol!("algebraic_pool_function");
        let call = f.call(9);
        let atoms = build_algebraic_letter_atom_list([call.clone()], 1);
        assert_eq!(atoms[0], call);
        let ctx = PolyCtx::from_indeterminates(atoms).unwrap();
        assert_eq!(ctx.variable_atom(0).unwrap(), call);
    }

    #[test]
    fn request_sessions_serialize_reset_through_result_observation() {
        use std::sync::mpsc;
        use std::thread;
        use std::time::Duration;

        let first = begin_algebraic_letter_session().unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (entered_tx, entered_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            started_tx.send(()).unwrap();
            let _second = begin_algebraic_letter_session().unwrap();
            entered_tx.send(()).unwrap();
        });

        started_rx.recv().unwrap();
        assert!(entered_rx.recv_timeout(Duration::from_millis(25)).is_err());
        drop(first);
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        worker.join().unwrap();
    }
}
