//! Reuse polynomial contexts by their ordered variable lists.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use super::PolyCtx;
use crate::error::{Error, Result};

/// Thread-safe weak interner for Symbolica polynomial contexts.
///
/// The weak values avoid turning every transient integration context into
/// process-lifetime state.  While a context is live, repeated requests return
/// the same [`Arc`], allowing downstream caches to use pointer identity.
#[derive(Debug, Default)]
pub struct ContextInterner {
    contexts: Mutex<HashMap<Vec<String>, Weak<PolyCtx>>>,
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
        let mut contexts = self
            .contexts
            .lock()
            .map_err(|_| Error::InvalidInput("polynomial context interner is poisoned".into()))?;
        if let Some(existing) = contexts.get(&names).and_then(Weak::upgrade) {
            return Ok(existing);
        }
        let context = PolyCtx::new(names.clone())?;
        contexts.insert(names, Arc::downgrade(&context));
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
        contexts.retain(|_, value| value.strong_count() != 0);
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
    use super::*;

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
}
