use std::sync::Arc;

use crate::core::{PolyCtx, Rat};
use crate::error::{Error, Result};

fn checked_integer_power(base: i64, exponent: i64) -> Result<i64> {
    if exponent <= 0 {
        return Err(Error::InvalidInput(format!(
            "multiple-polylogarithm indices must be positive, got {exponent}"
        )));
    }

    let mut result = 1_i64;
    for _ in 0..exponent {
        result = result.checked_mul(base).ok_or_else(|| {
            Error::InvalidInput(format!(
                "integer power {base}^{exponent} does not fit in i64"
            ))
        })?;
    }
    Ok(result)
}

fn validate_contexts(zs: &[Rat]) -> Result<Arc<PolyCtx>> {
    let ctx = zs
        .first()
        .ok_or_else(|| Error::InvalidInput("mpl_sum requires at least one argument".into()))?
        .ctx()
        .clone();
    if zs.iter().skip(1).any(|z| !z.ctx().is_compatible_with(&ctx)) {
        return Err(Error::ContextMismatch);
    }
    Ok(ctx)
}

/// Truncated defining sum of a multiple polylogarithm.
///
/// This is mathematically identical to HyperFLINT's recursive implementation,
/// but caches every prefix sum.  Consequently a depth-`d`, order-`n` sum takes
/// `O(d n)` rational operations instead of recursively recomputing sub-sums.
pub fn mpl_sum(ns: &[i64], zs: &[Rat], max_n: i64) -> Result<Rat> {
    let ctx = validate_contexts(zs)?;
    if ns.len() != zs.len() {
        return Err(Error::InvalidInput(format!(
            "mpl_sum received {} indices and {} arguments",
            ns.len(),
            zs.len()
        )));
    }
    if ns.iter().any(|&index| index <= 0) {
        return Err(Error::InvalidInput(
            "multiple-polylogarithm indices must be positive".into(),
        ));
    }
    if max_n < 1 {
        return Ok(Rat::zero(ctx));
    }

    let order = usize::try_from(max_n)
        .map_err(|_| Error::InvalidInput(format!("series order {max_n} is too large")))?;
    // prev[k] is the depth-(i-1) sum truncated at k.  At depth zero it
    // is one for every k, including k=0, exactly matching the recursive base.
    let mut previous = vec![Rat::one(ctx.clone()); order + 1];

    for (&index, z) in ns.iter().zip(zs) {
        let mut current = vec![Rat::zero(ctx.clone()); order + 1];
        let mut cumulative = Rat::zero(ctx.clone());
        let mut z_power = Rat::one(ctx.clone());
        for k in 1..=order {
            z_power = z_power.try_mul(z)?;
            let divisor = checked_integer_power(k as i64, index)?;
            let term = previous[k - 1]
                .try_mul(&z_power)?
                .try_div(&Rat::from_int(ctx.clone(), divisor))?;
            cumulative = cumulative.try_add(&term)?;
            current[k] = cumulative.clone();
        }
        previous = current;
    }

    Ok(previous[order].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_one_is_a_truncated_polylogarithm() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let x = Rat::parse(ctx.clone(), "x").unwrap();
        let sum = mpl_sum(&[1], &[x], 3).unwrap();
        assert_eq!(sum, Rat::parse(ctx, "x+x^2/2+x^3/3").unwrap());
    }

    #[test]
    fn depth_two_respects_strictly_nested_bounds() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let x = Rat::parse(ctx.clone(), "x").unwrap();
        let y = Rat::parse(ctx.clone(), "y").unwrap();
        let sum = mpl_sum(&[1, 1], &[x, y], 3).unwrap();
        assert_eq!(sum, Rat::parse(ctx, "x*y^2/2+x*y^3/3+x^2*y^3/6").unwrap());
    }

    #[test]
    fn validates_shape_indices_and_integer_overflow() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let x = Rat::parse(ctx, "x").unwrap();
        assert!(mpl_sum(&[1, 2], std::slice::from_ref(&x), 2).is_err());
        assert!(mpl_sum(&[0], std::slice::from_ref(&x), 2).is_err());
        assert!(mpl_sum(&[63], &[x], 2).is_err());
    }
}
