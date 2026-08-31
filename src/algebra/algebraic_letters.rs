//! Serialized algebraic-letter pairs and their Vieta reductions.
//!
//! A pair `Wm_i`, `Wp_i` denotes the two roots of a quadratic in one
//! selected variable.  The registry keeps the original quadratic and its
//! Vieta data while rational expressions continue to use ordinary variables
//! in their [`crate::core::PolyCtx`].

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use symbolica::prelude::Atom;

use crate::core::{Poly, PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::reduce::{MzvReductionTable, build_mzv_atom_list, substitute_var_rat};
use crate::symbols::{AlgebraicAtoms, algebraic_atoms};

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

#[derive(Default)]
struct TableState {
    entries: Vec<AlgebraicLetterEntry>,
    content_index: HashMap<String, usize>,
}

static TABLE: OnceLock<Mutex<TableState>> = OnceLock::new();
static SESSION: OnceLock<Mutex<()>> = OnceLock::new();

thread_local! {
    static SESSION_DEPTH: Cell<usize> = const { Cell::new(0) };
}

fn table() -> &'static Mutex<TableState> {
    TABLE.get_or_init(|| Mutex::new(TableState::default()))
}

fn lock_table() -> Result<MutexGuard<'static, TableState>> {
    table()
        .lock()
        .map_err(|_| Error::InvalidInput("algebraic-letter registry lock is poisoned".into()))
}

fn session_mutex() -> &'static Mutex<()> {
    SESSION.get_or_init(|| Mutex::new(()))
}

/// A re-entrant guard that prevents algebraic-letter state from interleaving
/// across independent integrations.
///
/// The guard is deliberately `!Send` through its `MutexGuard`: every session
/// must be entered and left on one thread. Internal table operations join an
/// existing session re-entrantly.
pub struct AlgebraicLetterSession {
    root_guard: Option<MutexGuard<'static, ()>>,
}

impl Drop for AlgebraicLetterSession {
    fn drop(&mut self) {
        SESSION_DEPTH.with(|depth| {
            let current = depth.get();
            debug_assert!(current > 0);
            depth.set(current.saturating_sub(1));
        });
        // Make the protected lifetime explicit; the field drops immediately
        // after this method and releases the process-wide session mutex.
        let _ = self.root_guard.as_ref();
    }
}

fn enter_session(reset: bool) -> Result<AlgebraicLetterSession> {
    let nested = SESSION_DEPTH.with(|depth| depth.get() > 0);
    if nested {
        if reset {
            return Err(Error::InvalidInput(
                "cannot reset the algebraic-letter table from a nested session".into(),
            ));
        }
        SESSION_DEPTH.with(|depth| depth.set(depth.get() + 1));
        return Ok(AlgebraicLetterSession { root_guard: None });
    }

    let guard = session_mutex()
        .lock()
        .map_err(|_| Error::InvalidInput("algebraic-letter session lock is poisoned".into()))?;
    SESSION_DEPTH.with(|depth| depth.set(1));
    let session = AlgebraicLetterSession {
        root_guard: Some(guard),
    };
    if reset {
        clear_table_state()?;
    }
    Ok(session)
}

/// Start a fresh request-scoped algebraic-letter session.
///
/// The table is cleared while the exclusive session is held. Keep the
/// returned guard alive through result/table serialization.
pub fn begin_algebraic_letter_session() -> Result<AlgebraicLetterSession> {
    enter_session(true)
}

pub(crate) fn join_algebraic_letter_session() -> Result<AlgebraicLetterSession> {
    enter_session(false)
}

fn clear_table_state() -> Result<()> {
    let mut state = lock_table()?;
    state.entries.clear();
    state.content_index.clear();
    Ok(())
}

/// Compatibility facade for the upstream singleton-style API.
#[derive(Clone, Copy, Debug, Default)]
pub struct AlgebraicLetterTable;

impl AlgebraicLetterTable {
    pub fn global() -> &'static Self {
        static FACADE: AlgebraicLetterTable = AlgebraicLetterTable;
        &FACADE
    }

    pub fn allocate(&self, polynomial: &Poly, var_idx: usize) -> Result<usize> {
        algebraic_letters_allocate(polynomial, var_idx)
    }

    pub fn at(&self, idx: usize) -> Result<AlgebraicLetterEntry> {
        let _session = join_algebraic_letter_session()?;
        let state = lock_table()?;
        if idx == 0 || idx > state.entries.len() {
            return Err(Error::InvalidInput(format!(
                "AlgebraicLetterTable::at: index {idx} is outside 1..={}",
                state.entries.len()
            )));
        }
        Ok(state.entries[idx - 1].clone())
    }

    pub fn size(&self) -> Result<usize> {
        algebraic_letters_size()
    }

    pub fn clear(&self) -> Result<()> {
        algebraic_letters_clear()
    }

    pub fn entries(&self) -> Result<Vec<AlgebraicLetterEntry>> {
        algebraic_letters_show()
    }
}

/// Clear all allocated pairs.  The next allocation receives index one.
pub fn algebraic_letters_clear() -> Result<()> {
    let _session = join_algebraic_letter_session()?;
    clear_table_state()
}

/// Return a stable insertion-order snapshot of the registry.
pub fn algebraic_letters_show() -> Result<Vec<AlgebraicLetterEntry>> {
    let _session = join_algebraic_letter_session()?;
    Ok(lock_table()?.entries.clone())
}

pub fn algebraic_letters_size() -> Result<usize> {
    let _session = join_algebraic_letter_session()?;
    Ok(lock_table()?.entries.len())
}

/// Allocate (or content-deduplicate) one quadratic root pair.
pub fn algebraic_letters_allocate(polynomial: &Poly, var_idx: usize) -> Result<usize> {
    let _session = join_algebraic_letter_session()?;
    let degree = polynomial.degree(var_idx)?;
    if degree != 2 {
        return Err(Error::InvalidInput(format!(
            "AlgebraicLetterTable::allocate: polynomial must have degree 2 in variable `{}`, got degree {degree}",
            polynomial.ctx().vars()[var_idx]
        )));
    }

    let dedup_key = format!("{var_idx}|{polynomial}");
    let mut state = lock_table()?;
    if let Some(&idx) = state.content_index.get(&dedup_key) {
        return Ok(idx);
    }

    let lc_poly = polynomial.coefficient_of(var_idx, 2)?;
    let b_poly = polynomial.coefficient_of(var_idx, 1)?;
    let c_poly = polynomial.coefficient_of(var_idx, 0)?;
    let lc = Rat::from_poly(lc_poly.clone());
    let b = Rat::from_poly(b_poly.clone());
    let c = Rat::from_poly(c_poly.clone());
    let sum_value = b.negated().try_div(&lc)?;
    let product_value = c.try_div(&lc)?;

    let discriminant = polynomial.discriminant(var_idx)?;

    let idx = state.entries.len() + 1;
    state.entries.push(AlgebraicLetterEntry {
        idx,
        polynomial: polynomial.clone(),
        var_idx,
        lc,
        sum_value,
        product_value,
        discriminant,
    });
    state.content_index.insert(dedup_key, idx);
    Ok(idx)
}

/// Append a deterministic pool of registered algebraic indeterminates.
pub fn build_algebraic_letter_atom_list(
    variables: impl IntoIterator<Item = Atom>,
    pool_size: usize,
) -> Vec<Atom> {
    let mut output = Vec::new();
    let mut add = |atom: Atom| {
        if !output.contains(&atom) {
            output.push(atom);
        }
    };
    for variable in variables {
        add(variable);
    }
    for idx in 1..=pool_size {
        let atoms = algebraic_atoms(
            u32::try_from(idx).expect("algebraic-letter pool index must fit in u32"),
        );
        add(atoms.minus);
        add(atoms.plus);
        add(atoms.ratio);
        add(atoms.sqrt_discriminant);
    }
    output
}

/// Combine user, MZV, and algebraic-letter registered indeterminates.
pub fn build_full_atom_list(
    reductions: &MzvReductionTable,
    variables: impl IntoIterator<Item = Atom>,
    pool_size: usize,
) -> Result<Vec<Atom>> {
    Ok(build_algebraic_letter_atom_list(
        build_mzv_atom_list(reductions, variables)?,
        pool_size,
    ))
}

fn find_var_idx(ctx: &PolyCtx, atom: &Atom) -> Option<usize> {
    ctx.index_of_indeterminate(atom.as_view())
}

fn algebraic_atom_total(value: &Rat, entries: &[AlgebraicLetterEntry]) -> Result<i64> {
    let mut total = 0_i64;
    for entry in entries {
        let atoms = entry.atoms();
        for atom in [&atoms.minus, &atoms.plus, &atoms.ratio] {
            let Some(variable) = find_var_idx(value.ctx(), atom) else {
                continue;
            };
            let numerator_degree = value.numerator().degree(variable)?.max(0);
            let denominator_degree = value.denominator().degree(variable)?.max(0);
            total = total
                .checked_add(numerator_degree)
                .and_then(|sum| sum.checked_add(denominator_degree))
                .ok_or_else(|| Error::InvalidInput("algebraic atom count overflowed".into()))?;
        }
    }
    Ok(total)
}

/// Replace a shrinking `Wm_i/Wp_i` ratio by `WmOverWp_i`.
pub fn combine_wm_wp_ratios(value: &Rat) -> Result<Rat> {
    let entries = algebraic_letters_show()?;
    let mut current = value.clone();
    for entry in &entries {
        let atoms = entry.atoms();
        let (Some(wm), Some(wp), Some(ratio)) = (
            find_var_idx(value.ctx(), &atoms.minus),
            find_var_idx(value.ctx(), &atoms.plus),
            find_var_idx(value.ctx(), &atoms.ratio),
        ) else {
            continue;
        };

        if current.numerator().degree(wm)? <= 0 || current.denominator().degree(wp)? <= 0 {
            continue;
        }
        let replacement = Rat::from_poly(Poly::generator(value.ctx().clone(), ratio)?)
            .try_mul(&Rat::from_poly(Poly::generator(value.ctx().clone(), wp)?))?;
        let candidate = substitute_var_rat(&current, wm, &replacement)?;
        if algebraic_atom_total(&candidate, &entries)? < algebraic_atom_total(&current, &entries)? {
            current = candidate;
        }
    }
    Ok(current)
}

/// Materialize every `Wm_i`/`Wp_i` through a symbolic square-root atom.
pub fn back_substitute(value: &Rat) -> Result<Rat> {
    let entries = algebraic_letters_show()?;
    let half =
        Rat::from_int(value.ctx().clone(), 1).try_div(&Rat::from_int(value.ctx().clone(), 2))?;
    let mut current = value.clone();

    for entry in &entries {
        let atoms = entry.atoms();
        let wm = find_var_idx(value.ctx(), &atoms.minus);
        let wp = find_var_idx(value.ctx(), &atoms.plus);
        let sqrt_disc = find_var_idx(value.ctx(), &atoms.sqrt_discriminant);
        let (Some(wm), Some(wp), Some(sqrt_disc)) = (wm, wp, sqrt_disc) else {
            return Err(Error::InvalidInput(format!(
                "back_substitute: context is missing registered Wm/Wp/sqrt_disc atoms for pair {} — call build_algebraic_letter_atom_list when constructing the context",
                entry.idx
            )));
        };

        let sqrt_disc = Rat::from_poly(Poly::generator(value.ctx().clone(), sqrt_disc)?);
        let sum_half = entry.sum_value.try_mul(&half)?;
        let sqrt_over_two_lc = sqrt_disc.try_mul(&half)?.try_div(&entry.lc)?;
        let wm_replacement = sum_half.try_sub(&sqrt_over_two_lc)?;
        let wp_replacement = sum_half.try_add(&sqrt_over_two_lc)?;
        current = substitute_var_rat(&current, wm, &wm_replacement)?;
        current = substitute_var_rat(&current, wp, &wp_replacement)?;
    }
    Ok(current)
}

fn reduce_var_via_recurrence(
    value: &Rat,
    variable: usize,
    sum_value: &Rat,
    product_value: &Rat,
) -> Result<Rat> {
    let max_degree = value.numerator().degree(variable)?;
    if max_degree < 2 {
        return Ok(value.clone());
    }

    let ctx = value.ctx().clone();
    let zero = Rat::zero(ctx.clone());
    let one = Rat::one(ctx.clone());
    let mut upper = Vec::with_capacity(max_degree as usize + 1);
    let mut lower = Vec::with_capacity(max_degree as usize + 1);
    upper.push(zero.clone());
    lower.push(one.clone());
    upper.push(one);
    lower.push(zero.clone());

    for degree in 2..=max_degree as usize {
        let next_upper = sum_value
            .try_mul(&upper[degree - 1])?
            .try_sub(&product_value.try_mul(&upper[degree - 2])?)?;
        let next_lower = sum_value
            .try_mul(&lower[degree - 1])?
            .try_sub(&product_value.try_mul(&lower[degree - 2])?)?;
        upper.push(next_upper);
        lower.push(next_lower);
    }

    let mut coefficient_of_var = zero.clone();
    let mut free_part = zero;
    for degree in 0..=max_degree {
        let coefficient = value.numerator().coefficient_of(variable, degree)?;
        if coefficient.is_zero() {
            continue;
        }
        let coefficient = Rat::new(coefficient, value.denominator().clone())?;
        coefficient_of_var =
            coefficient_of_var.try_add(&coefficient.try_mul(&upper[degree as usize])?)?;
        free_part = free_part.try_add(&coefficient.try_mul(&lower[degree as usize])?)?;
    }

    let generator = Rat::from_poly(Poly::generator(ctx, variable)?);
    free_part.try_add(&coefficient_of_var.try_mul(&generator)?)
}

/// Reduce algebraic atoms with the same per-pair Vieta normal form as Mma.
pub fn simplify_with_vieta(value: &Rat) -> Result<Rat> {
    let entries = algebraic_letters_show()?;
    if entries.is_empty() {
        return Ok(value.clone());
    }

    // HyperIntica gives up globally when any allocated Wm/Wp occurs in the
    // denominator.  Partial fractions may be used by callers to clear it.
    for entry in &entries {
        let atoms = entry.atoms();
        for atom in [&atoms.minus, &atoms.plus] {
            if let Some(variable) = find_var_idx(value.ctx(), atom)
                && value.denominator().degree(variable)? > 0
            {
                return Ok(value.clone());
            }
        }
    }

    let half =
        Rat::from_int(value.ctx().clone(), 1).try_div(&Rat::from_int(value.ctx().clone(), 2))?;
    let mut expression = value.clone();
    for entry in &entries {
        let atoms = entry.atoms();
        let (Some(wm), Some(wp)) = (
            find_var_idx(value.ctx(), &atoms.minus),
            find_var_idx(value.ctx(), &atoms.plus),
        ) else {
            continue;
        };

        let mut expanded =
            reduce_var_via_recurrence(&expression, wm, &entry.sum_value, &entry.product_value)?;
        expanded =
            reduce_var_via_recurrence(&expanded, wp, &entry.sum_value, &entry.product_value)?;

        let numerator = expanded.numerator();
        let denominator = expanded.denominator();
        let num_no_wm = numerator.coefficient_of(wm, 0)?;
        let num_wm = numerator.coefficient_of(wm, 1)?;
        let a = Rat::new(num_no_wm.coefficient_of(wp, 0)?, denominator.clone())?;
        let c = Rat::new(num_no_wm.coefficient_of(wp, 1)?, denominator.clone())?;
        let b = Rat::new(num_wm.coefficient_of(wp, 0)?, denominator.clone())?;
        let d = Rat::new(num_wm.coefficient_of(wp, 1)?, denominator.clone())?;
        let collapsed_constant = a.try_add(&d.try_mul(&entry.product_value)?)?;

        // This is the upstream PossibleZeroQ gate.  If b+c is non-zero, the
        // per-pair candidate is discarded even though a more aggressive
        // quotient-ring simplifier could reduce it.
        if !b.try_add(&c)?.is_zero() {
            continue;
        }

        let wm_rat = Rat::from_poly(Poly::generator(value.ctx().clone(), wm)?);
        let wp_rat = Rat::from_poly(Poly::generator(value.ctx().clone(), wp)?);
        let antisymmetric = b
            .try_sub(&c)?
            .try_mul(&half)?
            .try_mul(&wm_rat.try_sub(&wp_rat)?)?;
        expression = collapsed_constant.try_add(&antisymmetric)?;
    }
    Ok(expression)
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
