//! Collision-safe structural indices for LR hot paths.
//!
//! Symbolica's canonical polynomial hash is useful as a bucket accelerator,
//! but a `u64` digest is never an equality proof. These helpers retain the
//! complete [`Poly`] values (or indices into an owning vector) and confirm
//! native structural equality inside every matching bucket.

use std::collections::HashMap;
use std::hash::Hash;

use crate::core::{Poly, poly_bucket_digest, structural_bucket_digest_by};

/// An append-only, collision-safe index into an external polynomial vector.
///
/// Indices remain valid only while `values` is append-only. This is exactly
/// the ownership model of LR deduplication ledgers and the factor-table
/// interner.
#[derive(Clone, Debug, Default)]
pub(crate) struct PolyIndex {
    buckets: HashMap<u64, Vec<usize>>,
}

impl PolyIndex {
    fn find_in_bucket(
        &self,
        values: &[Poly],
        candidate: &Poly,
        bucket_digest: u64,
    ) -> Option<usize> {
        self.buckets.get(&bucket_digest).and_then(|bucket| {
            bucket
                .iter()
                .copied()
                .find(|&index| values.get(index).is_some_and(|value| value == candidate))
        })
    }

    /// Insert `candidate` when it is structurally new, returning its stable
    /// index and whether an insertion occurred.
    pub(crate) fn insert(&mut self, values: &mut Vec<Poly>, candidate: Poly) -> (usize, bool) {
        let bucket_digest = poly_bucket_digest(&candidate);
        self.insert_in_bucket(values, candidate, bucket_digest)
    }

    fn insert_in_bucket(
        &mut self,
        values: &mut Vec<Poly>,
        candidate: Poly,
        bucket_digest: u64,
    ) -> (usize, bool) {
        if let Some(index) = self.find_in_bucket(values, &candidate, bucket_digest) {
            return (index, false);
        }
        let index = values.len();
        values.push(candidate);
        self.buckets.entry(bucket_digest).or_default().push(index);
        (index, true)
    }

    pub(crate) fn clear(&mut self) {
        self.buckets.clear();
    }
}

#[derive(Clone, Debug)]
struct PolySliceEntry<V> {
    variable: usize,
    input: Box<[Poly]>,
    value: V,
}

/// A collision-safe cache keyed by `(variable, ordered polynomial slice)`.
///
/// The digest chooses a bucket only. Every hit compares the full variable and
/// polynomial slice, so even an adversarial digest collision cannot return a
/// value for a different reduction state.
#[derive(Clone, Debug)]
pub(crate) struct PolySliceCache<V> {
    buckets: HashMap<u64, Vec<PolySliceEntry<V>>>,
}

impl<V> Default for PolySliceCache<V> {
    fn default() -> Self {
        Self {
            buckets: HashMap::new(),
        }
    }
}

impl<V> PolySliceCache<V> {
    pub(crate) fn get(&self, variable: usize, input: &[Poly]) -> Option<&V> {
        self.get_in_bucket(variable, input, poly_slice_bucket_digest(variable, input))
    }

    fn get_in_bucket(&self, variable: usize, input: &[Poly], bucket_digest: u64) -> Option<&V> {
        self.buckets.get(&bucket_digest).and_then(|bucket| {
            bucket
                .iter()
                .find(|entry| entry.variable == variable && entry.input.as_ref() == input)
                .map(|entry| &entry.value)
        })
    }

    pub(crate) fn insert(&mut self, variable: usize, input: &[Poly], value: V) {
        self.insert_in_bucket(
            variable,
            input,
            value,
            poly_slice_bucket_digest(variable, input),
        );
    }

    fn insert_in_bucket(&mut self, variable: usize, input: &[Poly], value: V, bucket_digest: u64) {
        let bucket = self.buckets.entry(bucket_digest).or_default();
        if let Some(entry) = bucket
            .iter_mut()
            .find(|entry| entry.variable == variable && entry.input.as_ref() == input)
        {
            entry.value = value;
            return;
        }
        bucket.push(PolySliceEntry {
            variable,
            input: input.to_vec().into_boxed_slice(),
            value,
        });
    }
}

fn poly_slice_bucket_digest(variable: usize, input: &[Poly]) -> u64 {
    structural_bucket_digest_by(0x4859_5045_4c52_5354_u64, |state| {
        variable.hash(state);
        input.len().hash(state);
        // Every polynomial in an LR slice normally shares one context.
        // Hash it once instead of revisiting that variable map for every
        // entry, then hash only the canonical Symbolica payloads. Full `Poly`
        // equality (including every context in a malformed mixed slice) is
        // still checked on every hit.
        if let Some(first) = input.first() {
            first.ctx().native_variables().hash(state);
        }
        for polynomial in input {
            polynomial.hash_canonical_payload(state);
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::core::PolyCtx;

    use super::*;

    fn parse(ctx: &Arc<PolyCtx>, expression: &str) -> Poly {
        Poly::parse(ctx.clone(), expression).unwrap()
    }

    #[test]
    fn index_confirms_full_values_inside_an_adversarial_collision_bucket() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let x = parse(&ctx, "x");
        let y = parse(&ctx, "y");
        let expanded = parse(&ctx, "(x+y)^2");
        let canonical = parse(&ctx, "x^2+2*x*y+y^2");
        let forced_digest = 7;
        let mut values = Vec::new();
        let mut index = PolyIndex::default();

        assert_eq!(
            index.insert_in_bucket(&mut values, x.clone(), forced_digest),
            (0, true)
        );
        assert_eq!(
            index.insert_in_bucket(&mut values, y.clone(), forced_digest),
            (1, true)
        );
        assert_eq!(
            index.insert_in_bucket(&mut values, x, forced_digest),
            (0, false)
        );
        assert_eq!(
            index.insert_in_bucket(&mut values, expanded, forced_digest),
            (2, true)
        );
        assert_eq!(
            index.insert_in_bucket(&mut values, canonical, forced_digest),
            (2, false)
        );
        assert_eq!(values.len(), 3);
    }

    #[test]
    fn slice_cache_never_treats_a_digest_as_an_equality_proof() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let x = parse(&ctx, "x");
        let y = parse(&ctx, "y");
        let forced_digest = 11;
        let mut cache = PolySliceCache::default();

        cache.insert_in_bucket(0, std::slice::from_ref(&x), "x@0", forced_digest);
        cache.insert_in_bucket(0, std::slice::from_ref(&y), "y@0", forced_digest);
        cache.insert_in_bucket(1, std::slice::from_ref(&x), "x@1", forced_digest);

        assert_eq!(
            cache.get_in_bucket(0, std::slice::from_ref(&x), forced_digest),
            Some(&"x@0")
        );
        assert_eq!(
            cache.get_in_bucket(0, std::slice::from_ref(&y), forced_digest),
            Some(&"y@0")
        );
        assert_eq!(
            cache.get_in_bucket(1, std::slice::from_ref(&x), forced_digest),
            Some(&"x@1")
        );
        assert_eq!(
            cache.get_in_bucket(1, std::slice::from_ref(&y), forced_digest),
            None
        );
    }
}
