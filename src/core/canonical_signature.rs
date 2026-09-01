//! Allocation-free bucket digests for exact algebra values.
//!
//! A digest in this module is never a semantic key or an equality proof. The
//! module is private and exposes only a crate-private polynomial bucket
//! accelerator. Every consumer must retain and compare complete canonical
//! [`Poly`] values inside a matching bucket.

use std::hash::Hash;

use super::{Poly, structural_digest::structural_bucket_digest_by};

/// Hash Symbolica's canonical coefficient and exponent arrays directly.
///
/// Unlike a `to_string()`-based accelerator, this performs no formatting and
/// no heap allocation. Equal `Poly` values in compatible contexts have equal
/// digests. The inverse is deliberately not promised: callers must compare
/// the complete `Poly` values within the selected bucket.
pub(crate) fn poly_bucket_digest(poly: &Poly) -> u64 {
    structural_bucket_digest_by(0x4859_5045_5250_4f4c_u64, |state| {
        poly.ctx().native_variables().hash(state);
        poly.hash_canonical_payload(state);
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use symbolica::prelude::Symbol;

    use super::*;
    use crate::core::PolyCtx;

    fn context() -> Arc<PolyCtx> {
        PolyCtx::new(["x", "y"]).unwrap()
    }

    #[test]
    fn bucket_digest_follows_equal_canonical_values_without_formatting() {
        let ctx = context();
        let first = Poly::parse(ctx.clone(), "(x+y)^2").unwrap();
        let second = Poly::parse(ctx, "x^2+2*x*y+y^2").unwrap();

        assert_eq!(first, second);
        assert_eq!(poly_bucket_digest(&first), poly_bucket_digest(&second));
    }

    #[test]
    fn namespace_is_part_of_full_context_equality_even_if_a_bucket_collides() {
        let left_symbol = Symbol::parse("x", "digest_namespace_left").unwrap();
        let right_symbol = Symbol::parse("x", "digest_namespace_right").unwrap();
        let left_ctx = PolyCtx::from_symbols([left_symbol]).unwrap();
        let right_ctx = PolyCtx::from_symbols([right_symbol]).unwrap();
        let left = Poly::generator(left_ctx, 0).unwrap();
        let right = Poly::generator(right_ctx, 0).unwrap();

        assert_eq!(
            left_symbol.get_stripped_name(),
            right_symbol.get_stripped_name()
        );
        assert_ne!(left_symbol.get_namespace(), right_symbol.get_namespace());
        assert_ne!(left, right);

        // Model an adversarial digest collision explicitly. A bucket lookup
        // must still compare the complete context-sensitive `Poly` values.
        let forced_digest = 0_u64;
        let bucket = [(forced_digest, left)];
        assert!(
            bucket
                .iter()
                .filter(|(digest, _)| *digest == forced_digest)
                .all(|(_, candidate)| candidate != &right)
        );
    }

    #[test]
    fn constructor_specific_diagnostics_do_not_change_the_bucket_digest() {
        let symbol = Symbol::parse("x", "digest_shared_variable").unwrap();
        let qualified_ctx = PolyCtx::from_symbols([symbol]).unwrap();
        let stripped_ctx = PolyCtx::from_indeterminates([symbol.to_atom()]).unwrap();
        let qualified = Poly::generator(qualified_ctx, 0).unwrap();
        let stripped = Poly::generator(stripped_ctx, 0).unwrap();

        assert_ne!(qualified.ctx().vars(), stripped.ctx().vars());
        assert_eq!(qualified, stripped);
        assert_eq!(
            poly_bucket_digest(&qualified),
            poly_bucket_digest(&stripped)
        );
    }
}
