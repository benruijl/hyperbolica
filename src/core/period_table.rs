//! Dense process-stable identifiers for opaque period generators.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use crate::error::{Error, Result};

#[derive(Debug, Default)]
struct PeriodState {
    ids: HashMap<String, u32>,
    keys: Vec<String>,
}

/// Thread-safe registry used by `SymMonomial::period_powers`.
#[derive(Debug, Default)]
pub struct PeriodTable {
    state: RwLock<PeriodState>,
}

impl PeriodTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn id_for(&self, canonical_key: impl Into<String>) -> Result<u32> {
        let canonical_key = canonical_key.into();
        let mut state = self
            .state
            .write()
            .map_err(|_| Error::InvalidInput("period table is poisoned".into()))?;
        if let Some(&id) = state.ids.get(&canonical_key) {
            return Ok(id);
        }
        let id = u32::try_from(state.keys.len())
            .map_err(|_| Error::InvalidInput("period table exhausted u32 ids".into()))?;
        state.keys.push(canonical_key.clone());
        state.ids.insert(canonical_key, id);
        Ok(id)
    }

    /// Reverse lookup by value so concurrent insertion cannot invalidate a
    /// borrowed string.
    pub fn key_for(&self, id: u32) -> Result<String> {
        let state = self
            .state
            .read()
            .map_err(|_| Error::InvalidInput("period table is poisoned".into()))?;
        state
            .keys
            .get(id as usize)
            .cloned()
            .ok_or_else(|| Error::InvalidInput(format!("unknown period id {id}")))
    }

    pub fn len(&self) -> Result<usize> {
        let state = self
            .state
            .read()
            .map_err(|_| Error::InvalidInput("period table is poisoned".into()))?;
        Ok(state.keys.len())
    }

    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }
}

pub fn global_period_table() -> &'static PeriodTable {
    static TABLE: OnceLock<PeriodTable> = OnceLock::new();
    TABLE.get_or_init(PeriodTable::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_dense_and_stable() {
        let table = PeriodTable::new();
        let a = table.id_for("G[0,1]").unwrap();
        let b = table.id_for("mzv_3").unwrap();
        assert_eq!(a, 0);
        assert_eq!(b, 1);
        assert_eq!(table.id_for("G[0,1]").unwrap(), a);
        assert_eq!(table.key_for(b).unwrap(), "mzv_3");
    }
}
