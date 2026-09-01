//! Interned wide-context polynomial side table used by split scalars.

use std::collections::HashMap;
use std::sync::Arc;

use super::canonical_signature::poly_bucket_digest;
use super::{Poly, PolyCtx};
use crate::error::{Error, Result};

pub type ZwHandle = u32;
#[allow(clippy::upper_case_acronyms)]
pub type ZWHandle = ZwHandle;
pub const ZW_ONE: ZwHandle = 0;
pub const ZW_ZERO: ZwHandle = u32::MAX;
const ZW_OPAQUE_BIT: ZwHandle = 0x8000_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZwIntent {
    Numerator,
    Denominator,
}

/// Observable counters for sizing the interner and its operation caches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ZwStats {
    pub intern_calls: u64,
    pub intern_hits: u64,
    pub multiply_calls: u64,
    pub multiply_hits: u64,
    pub add_calls: u64,
    pub add_hits: u64,
    pub negate_calls: u64,
    pub negate_hits: u64,
}

/// Collision-safe polynomial interner with memoized commutative arithmetic.
///
/// A handle is scoped to one table.  The table is intentionally owned by an
/// integration context: persisting it across a regulator chain is what turns
/// repeated wide polynomial products into cheap integer lookups.
#[derive(Debug)]
pub struct ZwTable {
    ctx: Arc<PolyCtx>,
    zero: Poly,
    entries: Vec<Poly>,
    by_digest_bucket: HashMap<u64, Vec<ZwHandle>>,
    multiply_cache: HashMap<(ZwHandle, ZwHandle), ZwHandle>,
    add_cache: HashMap<(ZwHandle, ZwHandle), ZwHandle>,
    negate_cache: HashMap<ZwHandle, ZwHandle>,
    stats: ZwStats,
}

#[allow(clippy::upper_case_acronyms)]
pub type ZWTable = ZwTable;

impl ZwTable {
    pub fn new(ctx: Arc<PolyCtx>) -> Self {
        let zero = Poly::zero(ctx.clone());
        let one = Poly::one(ctx.clone());
        let one_digest = poly_bucket_digest(&one);
        let mut by_digest_bucket = HashMap::with_capacity(64);
        by_digest_bucket.insert(one_digest, vec![ZW_ONE]);
        Self {
            ctx,
            zero,
            entries: vec![one],
            by_digest_bucket,
            multiply_cache: HashMap::with_capacity(64),
            add_cache: HashMap::with_capacity(64),
            negate_cache: HashMap::with_capacity(32),
            stats: ZwStats::default(),
        }
    }

    pub fn ctx(&self) -> &Arc<PolyCtx> {
        &self.ctx
    }

    pub fn size(&self) -> usize {
        self.entries.len()
    }

    pub fn stats(&self) -> ZwStats {
        self.stats
    }

    pub fn total_terms(&self) -> usize {
        self.entries.iter().map(Poly::n_terms).sum()
    }

    pub fn intern(&mut self, polynomial: Poly, _intent: ZwIntent) -> Result<ZwHandle> {
        self.stats.intern_calls += 1;
        self.require_context(&polynomial)?;
        if polynomial.is_zero() {
            self.stats.intern_hits += 1;
            return Ok(ZW_ZERO);
        }
        if polynomial.is_one() {
            self.stats.intern_hits += 1;
            return Ok(ZW_ONE);
        }

        let digest = poly_bucket_digest(&polynomial);
        self.intern_nontrivial_in_digest_bucket(polynomial, digest)
    }

    /// Finish interning a validated, nonzero, nonunit polynomial in one
    /// digest bucket. The complete polynomial comparison below is the
    /// equality proof; `digest` only selects candidates to inspect.
    fn intern_nontrivial_in_digest_bucket(
        &mut self,
        polynomial: Poly,
        digest: u64,
    ) -> Result<ZwHandle> {
        if let Some(handles) = self.by_digest_bucket.get(&digest) {
            for &handle in handles {
                if self.entries[handle as usize].equal(&polynomial) {
                    self.stats.intern_hits += 1;
                    return Ok(handle);
                }
            }
        }

        if self.entries.len() >= ZW_OPAQUE_BIT as usize {
            return Err(Error::InvalidInput(
                "wide polynomial handle space exhausted".into(),
            ));
        }
        let handle = self.entries.len() as ZwHandle;
        self.entries.push(polynomial);
        self.by_digest_bucket
            .entry(digest)
            .or_default()
            .push(handle);
        Ok(handle)
    }

    #[cfg(test)]
    fn intern_with_forced_digest(
        &mut self,
        polynomial: Poly,
        intent: ZwIntent,
        digest: u64,
    ) -> Result<ZwHandle> {
        self.stats.intern_calls += 1;
        self.require_context(&polynomial)?;
        if polynomial.is_zero() || polynomial.is_one() {
            return self.intern(polynomial, intent);
        }
        self.intern_nontrivial_in_digest_bucket(polynomial, digest)
    }

    pub fn get(&self, handle: ZwHandle) -> Result<&Poly> {
        if handle == ZW_ZERO {
            return Ok(&self.zero);
        }
        if handle & ZW_OPAQUE_BIT != 0 {
            return Err(Error::InvalidInput(format!(
                "opaque wide polynomial handle {handle:#x} is unsupported"
            )));
        }
        self.entries
            .get(handle as usize)
            .ok_or_else(|| Error::InvalidInput(format!("unknown wide polynomial handle {handle}")))
    }

    pub fn multiply(&mut self, left: ZwHandle, right: ZwHandle) -> Result<ZwHandle> {
        self.stats.multiply_calls += 1;
        if left == ZW_ZERO || right == ZW_ZERO {
            return Ok(ZW_ZERO);
        }
        if left == ZW_ONE {
            return Ok(right);
        }
        if right == ZW_ONE {
            return Ok(left);
        }
        let key = sorted_pair(left, right);
        if let Some(&handle) = self.multiply_cache.get(&key) {
            self.stats.multiply_hits += 1;
            return Ok(handle);
        }
        let product = self.get(left)?.try_mul(self.get(right)?)?;
        let handle = self.intern(product, ZwIntent::Numerator)?;
        self.multiply_cache.insert(key, handle);
        Ok(handle)
    }

    pub fn add(&mut self, left: ZwHandle, right: ZwHandle) -> Result<ZwHandle> {
        self.stats.add_calls += 1;
        if left == ZW_ZERO {
            return Ok(right);
        }
        if right == ZW_ZERO {
            return Ok(left);
        }
        let key = sorted_pair(left, right);
        if let Some(&handle) = self.add_cache.get(&key) {
            self.stats.add_hits += 1;
            return Ok(handle);
        }
        let sum = self.get(left)?.try_add(self.get(right)?)?;
        let handle = self.intern(sum, ZwIntent::Numerator)?;
        self.add_cache.insert(key, handle);
        Ok(handle)
    }

    pub fn negate(&mut self, handle: ZwHandle) -> Result<ZwHandle> {
        self.stats.negate_calls += 1;
        if handle == ZW_ZERO {
            return Ok(ZW_ZERO);
        }
        if let Some(&negated) = self.negate_cache.get(&handle) {
            self.stats.negate_hits += 1;
            return Ok(negated);
        }
        let negated_polynomial = -self.get(handle)?;
        let negated = self.intern(negated_polynomial, ZwIntent::Numerator)?;
        self.negate_cache.insert(handle, negated);
        self.negate_cache.insert(negated, handle);
        Ok(negated)
    }

    /// Deterministically import another table and return its handle remap.
    pub fn merge_from(&mut self, secondary: &Self) -> Result<HashMap<ZwHandle, ZwHandle>> {
        if !self.ctx.is_compatible_with(&secondary.ctx) {
            return Err(Error::ContextMismatch);
        }
        let mut order = (1..secondary.entries.len()).collect::<Vec<_>>();
        order.sort_unstable_by(|&left, &right| {
            secondary.entries[left]
                .structural_cmp(&secondary.entries[right])
                .then_with(|| left.cmp(&right))
        });

        let mut remap = HashMap::with_capacity(secondary.entries.len());
        remap.insert(ZW_ONE, ZW_ONE);
        for index in order {
            let destination = self.intern(secondary.entries[index].clone(), ZwIntent::Numerator)?;
            remap.insert(index as ZwHandle, destination);
        }
        Ok(remap)
    }

    fn require_context(&self, polynomial: &Poly) -> Result<()> {
        if self.ctx.is_compatible_with(polynomial.ctx()) {
            Ok(())
        } else {
            Err(Error::ContextMismatch)
        }
    }
}

fn sorted_pair(left: ZwHandle, right: ZwHandle) -> (ZwHandle, ZwHandle) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use symbolica::prelude::Symbol;

    fn context() -> Arc<PolyCtx> {
        PolyCtx::new(["x", "s"]).unwrap()
    }

    #[test]
    fn interning_deduplicates_and_arithmetic_is_memoized() {
        let ctx = context();
        let mut table = ZwTable::new(ctx.clone());
        let x = Poly::parse(ctx.clone(), "x").unwrap();
        let s = Poly::parse(ctx.clone(), "s").unwrap();
        let hx = table.intern(x.clone(), ZwIntent::Numerator).unwrap();
        assert_eq!(table.intern(x, ZwIntent::Numerator).unwrap(), hx);
        let hs = table.intern(s, ZwIntent::Numerator).unwrap();
        let product = table.multiply(hx, hs).unwrap();
        assert_eq!(table.multiply(hs, hx).unwrap(), product);
        assert_eq!(
            table.get(product).unwrap(),
            &Poly::parse(ctx, "x*s").unwrap()
        );
        assert_eq!(table.stats().multiply_hits, 1);
        assert!(table.stats().intern_hits >= 1);
    }

    #[test]
    fn zero_and_one_use_sentinels() {
        let ctx = context();
        let mut table = ZwTable::new(ctx.clone());
        assert_eq!(
            table
                .intern(Poly::zero(ctx.clone()), ZwIntent::Numerator)
                .unwrap(),
            ZW_ZERO
        );
        assert_eq!(table.multiply(ZW_ONE, ZW_ZERO).unwrap(), ZW_ZERO);
        assert!(table.get(ZW_ZERO).unwrap().is_zero());
    }

    #[test]
    fn adversarial_digest_collision_still_compares_full_polynomials() {
        let ctx = context();
        let mut table = ZwTable::new(ctx.clone());
        let x = Poly::parse(ctx.clone(), "x").unwrap();
        let s = Poly::parse(ctx, "s").unwrap();
        let forced_digest = 0_u64;

        let x_handle = table
            .intern_with_forced_digest(x.clone(), ZwIntent::Numerator, forced_digest)
            .unwrap();
        let s_handle = table
            .intern_with_forced_digest(s.clone(), ZwIntent::Numerator, forced_digest)
            .unwrap();
        let repeated_x = table
            .intern_with_forced_digest(x.clone(), ZwIntent::Numerator, forced_digest)
            .unwrap();

        assert_ne!(x_handle, s_handle);
        assert_eq!(repeated_x, x_handle);
        assert_eq!(table.get(x_handle).unwrap(), &x);
        assert_eq!(table.get(s_handle).unwrap(), &s);
    }

    #[test]
    fn merge_order_is_structural_across_constructor_and_insertion_order() {
        let x = Symbol::parse("x", "zw_merge_structural").unwrap();
        let s = Symbol::parse("s", "zw_merge_structural").unwrap();
        let qualified_ctx = PolyCtx::from_symbols([x, s]).unwrap();
        let atom_ctx = PolyCtx::from_indeterminates([x.to_atom(), s.to_atom()]).unwrap();
        assert!(qualified_ctx.is_compatible_with(&atom_ctx));
        assert_ne!(qualified_ctx.vars(), atom_ctx.vars());

        let qualified_x = Poly::generator(qualified_ctx.clone(), 0).unwrap();
        let qualified_s = Poly::generator(qualified_ctx.clone(), 1).unwrap();
        let atom_x = Poly::generator(atom_ctx.clone(), 0).unwrap();
        let atom_s = Poly::generator(atom_ctx.clone(), 1).unwrap();

        let mut first_source = ZwTable::new(qualified_ctx.clone());
        let first_x = first_source
            .intern(qualified_x.clone(), ZwIntent::Numerator)
            .unwrap();
        let first_s = first_source
            .intern(qualified_s.clone(), ZwIntent::Numerator)
            .unwrap();
        let mut second_source = ZwTable::new(atom_ctx.clone());
        let second_s = second_source
            .intern(atom_s.clone(), ZwIntent::Numerator)
            .unwrap();
        let second_x = second_source
            .intern(atom_x.clone(), ZwIntent::Numerator)
            .unwrap();

        let mut first_destination = ZwTable::new(atom_ctx);
        let first_remap = first_destination.merge_from(&first_source).unwrap();
        let mut second_destination = ZwTable::new(qualified_ctx);
        let second_remap = second_destination.merge_from(&second_source).unwrap();

        assert_eq!(first_remap[&first_x], second_remap[&second_x]);
        assert_eq!(first_remap[&first_s], second_remap[&second_s]);
        assert_eq!(first_destination.size(), second_destination.size());
        for handle in 0..first_destination.size() as ZwHandle {
            assert_eq!(
                first_destination.get(handle).unwrap(),
                second_destination.get(handle).unwrap()
            );
        }
    }

    #[test]
    fn merge_order_ignores_adversarial_digest_buckets() {
        let ctx = context();
        let x = Poly::parse(ctx.clone(), "x+2").unwrap();
        let s = Poly::parse(ctx.clone(), "s+10").unwrap();
        let mut source = ZwTable::new(ctx.clone());
        source
            .intern_with_forced_digest(s.clone(), ZwIntent::Numerator, 0)
            .unwrap();
        source
            .intern_with_forced_digest(x.clone(), ZwIntent::Numerator, 0)
            .unwrap();

        let mut expected = [x, s];
        expected.sort_unstable_by(Poly::structural_cmp);

        let mut destination = ZwTable::new(ctx);
        destination.merge_from(&source).unwrap();
        assert_eq!(destination.get(1).unwrap(), &expected[0]);
        assert_eq!(destination.get(2).unwrap(), &expected[1]);
    }
}
