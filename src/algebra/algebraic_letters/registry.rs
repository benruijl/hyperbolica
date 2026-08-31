//! Request-scoped, deterministic algebraic-letter registry.

use std::cell::Cell;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::core::{Poly, Rat};
use crate::error::{Error, Result};

use super::AlgebraicLetterEntry;

#[derive(Default)]
struct TableState {
    entries: Vec<AlgebraicLetterEntry>,
    content_index: HashMap<(usize, u64), Vec<usize>>,
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

fn polynomial_digest(polynomial: &Poly) -> u64 {
    let mut hasher = DefaultHasher::new();
    polynomial.hash(&mut hasher);
    hasher.finish()
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
    algebraic_letters_allocate_with_digest(polynomial, var_idx, polynomial_digest(polynomial))
}

fn algebraic_letters_allocate_with_digest(
    polynomial: &Poly,
    var_idx: usize,
    digest: u64,
) -> Result<usize> {
    let _session = join_algebraic_letter_session()?;
    let degree = polynomial.degree(var_idx)?;
    if degree != 2 {
        return Err(Error::InvalidInput(format!(
            "AlgebraicLetterTable::allocate: polynomial must have degree 2 in variable `{}`, got degree {degree}",
            polynomial.ctx().vars()[var_idx]
        )));
    }

    let bucket_key = (var_idx, digest);
    let mut state = lock_table()?;
    if let Some(idx) = state.content_index.get(&bucket_key).and_then(|candidates| {
        candidates.iter().copied().find(|&idx| {
            state.entries[idx - 1].var_idx == var_idx
                && state.entries[idx - 1].polynomial == *polynomial
        })
    }) {
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
    state.content_index.entry(bucket_key).or_default().push(idx);
    Ok(idx)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use symbolica::prelude::Symbol;

    use super::*;
    use crate::core::PolyCtx;

    fn namespaced_context(namespace: &'static str) -> Arc<PolyCtx> {
        let symbol = Symbol::parse("x", namespace).unwrap();
        PolyCtx::from_indeterminates([symbol.to_atom()]).unwrap()
    }

    fn square_plus(ctx: &Arc<PolyCtx>, variable: usize, constant: i64) -> Poly {
        let generator = Poly::generator(ctx.clone(), variable).unwrap();
        generator
            .try_mul(&generator)
            .unwrap()
            .try_add(&Poly::from_int(ctx.clone(), constant))
            .unwrap()
    }

    #[test]
    fn structurally_namespaced_polynomials_do_not_share_an_id() {
        let _session = begin_algebraic_letter_session().unwrap();
        let left = square_plus(&namespaced_context("letter_registry_left"), 0, -2);
        let right = square_plus(&namespaced_context("letter_registry_right"), 0, -2);

        assert_eq!(left.to_string(), right.to_string());
        assert_ne!(left, right);
        assert_eq!(algebraic_letters_allocate(&left, 0).unwrap(), 1);
        assert_eq!(algebraic_letters_allocate(&right, 0).unwrap(), 2);
        assert_eq!(algebraic_letters_allocate(&left, 0).unwrap(), 1);
    }

    #[test]
    fn forced_digest_collisions_preserve_full_pair_identity_and_order() {
        let _session = begin_algebraic_letter_session().unwrap();
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let x_square = square_plus(&ctx, 0, 0);
        let y_square = square_plus(&ctx, 1, 0);
        let first = x_square
            .try_add(&y_square)
            .unwrap()
            .try_add(&Poly::from_int(ctx.clone(), -2))
            .unwrap();
        let second = x_square
            .try_add(&y_square)
            .unwrap()
            .try_add(&Poly::from_int(ctx, -3))
            .unwrap();

        assert_eq!(
            algebraic_letters_allocate_with_digest(&first, 1, 0).unwrap(),
            1
        );
        assert_eq!(
            algebraic_letters_allocate_with_digest(&second, 1, 0).unwrap(),
            2
        );
        assert_eq!(
            algebraic_letters_allocate_with_digest(&first, 0, 0).unwrap(),
            3
        );
        assert_eq!(
            algebraic_letters_allocate_with_digest(&first, 1, 0).unwrap(),
            1
        );

        let state = lock_table().unwrap();
        assert_eq!(
            state
                .entries
                .iter()
                .map(|entry| entry.idx)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(state.content_index.get(&(1, 0)).unwrap(), &[1, 2]);
        assert_eq!(state.content_index.get(&(0, 0)).unwrap(), &[3]);
    }
}
