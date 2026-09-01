//! Stable structural digest buckets shared by non-CAS domain algorithms.
//!
//! Digests in this module are accelerators and deterministic seed material,
//! never equality proofs. Semantic caches must retain complete typed values
//! and confirm their equality for every candidate selected from a bucket.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

const FNV1A_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Deterministic FNV-1a sink for structural [`Hash`] events.
///
/// Integer methods use explicit little-endian fixed widths; `usize` and
/// `isize` are widened to 64 bits. This keeps bucket/seed digests stable across
/// standard-library hasher changes and 32/64-bit targets.
#[derive(Clone, Debug)]
pub(crate) struct StableFnv1aHasher(u64);

impl Default for StableFnv1aHasher {
    fn default() -> Self {
        Self(FNV1A_OFFSET_BASIS)
    }
}

impl Hasher for StableFnv1aHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(FNV1A_PRIME);
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.write(&value.to_le_bytes());
    }

    fn write_u16(&mut self, value: u16) {
        self.write(&value.to_le_bytes());
    }

    fn write_u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    fn write_u128(&mut self, value: u128) {
        self.write(&value.to_le_bytes());
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    fn write_i8(&mut self, value: i8) {
        self.write(&value.to_le_bytes());
    }

    fn write_i16(&mut self, value: i16) {
        self.write(&value.to_le_bytes());
    }

    fn write_i32(&mut self, value: i32) {
        self.write(&value.to_le_bytes());
    }

    fn write_i64(&mut self, value: i64) {
        self.write(&value.to_le_bytes());
    }

    fn write_i128(&mut self, value: i128) {
        self.write(&value.to_le_bytes());
    }

    fn write_isize(&mut self, value: isize) {
        self.write_i64(value as i64);
    }
}

/// Hash a complete structural value into a domain-separated bucket selector.
pub(crate) fn structural_bucket_digest<T: Hash + ?Sized>(domain: u64, value: &T) -> u64 {
    structural_bucket_digest_by(domain, |state| value.hash(state))
}

/// Build a digest for a value without a direct [`Hash`] implementation.
pub(crate) fn structural_bucket_digest_by(
    domain: u64,
    hash_value: impl FnOnce(&mut StableFnv1aHasher),
) -> u64 {
    let mut state = StableFnv1aHasher::default();
    domain.hash(&mut state);
    hash_value(&mut state);
    state.finish()
}

/// Digest buckets containing stable indices into a caller-owned collection.
///
/// `find` requires an equality predicate, making complete collision
/// resolution explicit at every semantic lookup site.
#[derive(Clone, Debug, Default)]
pub(crate) struct DigestBuckets {
    buckets: HashMap<u64, Vec<usize>>,
}

impl DigestBuckets {
    pub(crate) fn find(
        &self,
        digest: u64,
        mut matches_complete_value: impl FnMut(usize) -> bool,
    ) -> Option<usize> {
        self.buckets.get(&digest).and_then(|bucket| {
            bucket
                .iter()
                .copied()
                .find(|&index| matches_complete_value(index))
        })
    }

    pub(crate) fn insert(&mut self, digest: u64, index: usize) {
        self.buckets.entry(digest).or_default().push(index);
    }

    #[cfg(test)]
    pub(crate) fn bucket_len(&self, digest: u64) -> usize {
        self.buckets.get(&digest).map_or(0, Vec::len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forced_bucket_collisions_still_require_the_full_predicate() {
        let mut buckets = DigestBuckets::default();
        let values = ["alpha", "beta", "alpha"];
        let forced_digest = 0;
        buckets.insert(forced_digest, 0);
        buckets.insert(forced_digest, 1);

        assert_eq!(
            buckets.find(forced_digest, |index| values[index] == values[2]),
            Some(0)
        );
        assert_eq!(
            buckets.find(forced_digest, |index| values[index] == "gamma"),
            None
        );
    }

    #[test]
    fn stable_digest_is_domain_separated_and_repeatable() {
        let first = structural_bucket_digest(1, &("x", 17_usize, -3_i64));
        let repeated = structural_bucket_digest(1, &("x", 17_usize, -3_i64));
        let other_domain = structural_bucket_digest(2, &("x", 17_usize, -3_i64));
        assert_eq!(first, repeated);
        assert_ne!(first, other_domain);
    }
}
