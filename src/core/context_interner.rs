//! Reuse polynomial contexts by their full ordered structural identities.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use super::{PolyCtx, structural_digest::structural_bucket_digest_by};
use crate::error::{Error, Result};

/// Thread-safe weak interner for Symbolica polynomial contexts.
///
/// The weak values avoid turning every transient integration context into
/// process-lifetime state.  While a context is live, repeated requests return
/// the same [`Arc`], allowing downstream caches to use pointer identity.
#[derive(Debug, Default)]
pub struct ContextInterner {
    contexts: Mutex<HashMap<u64, Vec<Weak<PolyCtx>>>>,
}

fn context_digest(context: &PolyCtx) -> u64 {
    structural_bucket_digest_by(0x4859_5045_5243_5458, |state| {
        context.native_variables().hash(state);
    })
}

fn same_context(left: &PolyCtx, right: &PolyCtx) -> bool {
    left.is_compatible_with(right)
}

impl ContextInterner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern<I, S>(&self, variables: I) -> Result<Arc<PolyCtx>>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let names = variables.into_iter().map(Into::into).collect::<Vec<_>>();
        self.intern_context(PolyCtx::new(names)?)
    }

    /// Intern an already constructed, potentially Atom-native context.
    ///
    /// The complete ordered Symbolica variable map participates in identity,
    /// so equal-looking symbols from different namespaces and distinct
    /// function/power indeterminates cannot alias. Diagnostic spellings are
    /// presentation-only and do not split one native ring into two contexts.
    pub fn intern_context(&self, context: Arc<PolyCtx>) -> Result<Arc<PolyCtx>> {
        self.intern_context_with_digest(context_digest(&context), context)
    }

    fn intern_context_with_digest(
        &self,
        digest: u64,
        context: Arc<PolyCtx>,
    ) -> Result<Arc<PolyCtx>> {
        let mut contexts = self
            .contexts
            .lock()
            .map_err(|_| Error::InvalidInput("polynomial context interner is poisoned".into()))?;

        let bucket = contexts.entry(digest).or_default();
        bucket.retain(|candidate| candidate.strong_count() != 0);
        if let Some(existing) = bucket
            .iter()
            .filter_map(Weak::upgrade)
            .find(|candidate| same_context(candidate, &context))
        {
            return Ok(existing);
        }
        bucket.push(Arc::downgrade(&context));
        Ok(context)
    }

    /// Number of currently live interned contexts.
    pub fn live_len(&self) -> Result<usize> {
        let contexts = self
            .contexts
            .lock()
            .map_err(|_| Error::InvalidInput("polynomial context interner is poisoned".into()))?;
        Ok(contexts
            .values()
            .flatten()
            .filter(|value| value.strong_count() != 0)
            .count())
    }

    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.live_len()? == 0)
    }

    /// Remove keys whose contexts no longer have any owners.
    pub fn prune(&self) -> Result<()> {
        let mut contexts = self
            .contexts
            .lock()
            .map_err(|_| Error::InvalidInput("polynomial context interner is poisoned".into()))?;
        contexts.retain(|_, bucket| {
            bucket.retain(|value| value.strong_count() != 0);
            !bucket.is_empty()
        });
        Ok(())
    }
}

/// Process-wide context interner used by protocol and integration frontends.
pub fn global_context_interner() -> &'static ContextInterner {
    static INTERNER: OnceLock<ContextInterner> = OnceLock::new();
    INTERNER.get_or_init(ContextInterner::new)
}

pub fn intern_poly_ctx<I, S>(variables: I) -> Result<Arc<PolyCtx>>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    global_context_interner().intern(variables)
}

#[cfg(test)]
mod tests {
    use symbolica::prelude::Symbol;

    use super::*;

    fn namespaced_context(namespace: &'static str) -> Arc<PolyCtx> {
        let symbol = Symbol::parse("x", namespace).unwrap();
        let function = Symbol::parse("f", namespace).unwrap();
        PolyCtx::from_indeterminates([symbol.to_atom(), function.call(1)]).unwrap()
    }

    #[test]
    fn identical_live_contexts_share_the_same_allocation() {
        let interner = ContextInterner::new();
        let first = interner.intern(["x", "y"]).unwrap();
        let second = interner.intern(["x", "y"]).unwrap();
        let reordered = interner.intern(["y", "x"]).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert!(!Arc::ptr_eq(&first, &reordered));
        assert_eq!(interner.live_len().unwrap(), 2);
    }

    #[test]
    fn dead_contexts_can_be_pruned() {
        let interner = ContextInterner::new();
        let context = interner.intern(["temporary"]).unwrap();
        drop(context);
        interner.prune().unwrap();
        assert!(interner.is_empty().unwrap());
    }

    #[test]
    fn atom_native_contexts_distinguish_symbol_namespaces() {
        let interner = ContextInterner::new();
        let left = namespaced_context("context_interner_left");
        let left_again = namespaced_context("context_interner_left");
        let right = namespaced_context("context_interner_right");

        assert_eq!(left.vars(), right.vars());
        assert_ne!(left.variable_map(), right.variable_map());

        let first = interner.intern_context(left).unwrap();
        let repeated = interner.intern_context(left_again).unwrap();
        let namespaced = interner.intern_context(right).unwrap();
        assert!(Arc::ptr_eq(&first, &repeated));
        assert!(!Arc::ptr_eq(&first, &namespaced));
        assert_eq!(interner.live_len().unwrap(), 2);
    }

    #[test]
    fn one_native_ring_interns_across_diagnostic_constructor_spellings() {
        let interner = ContextInterner::new();
        let symbol = Symbol::parse("x", "context_interner_shared").unwrap();
        let qualified = PolyCtx::from_symbols([symbol]).unwrap();
        let stripped = PolyCtx::from_indeterminates([symbol.to_atom()]).unwrap();
        assert_ne!(qualified.vars(), stripped.vars());
        assert!(qualified.is_compatible_with(&stripped));

        let first = interner.intern_context(qualified).unwrap();
        let repeated = interner.intern_context(stripped).unwrap();
        assert!(Arc::ptr_eq(&first, &repeated));
        assert_eq!(interner.live_len().unwrap(), 1);
    }

    #[test]
    fn forced_digest_collisions_still_compare_complete_contexts() {
        let interner = ContextInterner::new();
        let left = namespaced_context("context_collision_left");
        let left_again = namespaced_context("context_collision_left");
        let right = namespaced_context("context_collision_right");

        let first = interner.intern_context_with_digest(0, left).unwrap();
        let namespaced = interner.intern_context_with_digest(0, right).unwrap();
        let repeated = interner.intern_context_with_digest(0, left_again).unwrap();

        assert!(!Arc::ptr_eq(&first, &namespaced));
        assert!(Arc::ptr_eq(&first, &repeated));
        assert_eq!(interner.live_len().unwrap(), 2);
    }
}
