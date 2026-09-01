//! Wide rational function to narrow-polynomial/W-side-handle adapter.

use std::collections::BTreeMap;
use std::sync::Arc;

use symbolica::prelude::*;

use super::{Poly, PolyCtx, Rat, ZwHandle, ZwIntent, ZwTable};
use crate::error::{Error, Result};

/// Native-variable maps for a decomposition `wide = narrow + W-side`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FnIndexMaps {
    pub wide_to_narrow: Vec<Option<usize>>,
    pub narrow_to_wide: Vec<usize>,
}

pub fn build_fn_index_maps(wide: &PolyCtx, narrow: &PolyCtx) -> Result<FnIndexMaps> {
    let mut narrow_to_wide = Vec::with_capacity(narrow.len());
    let mut wide_to_narrow = vec![None; wide.len()];
    let wide_variables = wide.variable_map();
    let narrow_variables = narrow.variable_map();
    for (narrow_index, (name, variable)) in narrow
        .vars()
        .iter()
        .zip(narrow_variables.iter())
        .enumerate()
    {
        let wide_index = wide_variables
            .iter()
            .position(|candidate| candidate == variable)
            .ok_or_else(|| {
                Error::InvalidInput(format!(
                    "narrow Symbolica variable `{name}` is absent from the wide context"
                ))
            })?;
        narrow_to_wide.push(wide_index);
        wide_to_narrow[wide_index] = Some(narrow_index);
    }
    Ok(FnIndexMaps {
        wide_to_narrow,
        narrow_to_wide,
    })
}

/// One leaf of a split symbolic monomial.
///
/// It represents `num_n * table[num_zw] / table[den_zw]`, multiplied
/// by the symbolic powers carried alongside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymMonomialSplit {
    pub num_n: Poly,
    pub num_zw: ZwHandle,
    pub den_zw: ZwHandle,
    pub pi_power: i32,
    pub i_power: i32,
    pub log_powers: BTreeMap<i64, i32>,
    /// Powers of formal delta generators keyed by the wide-context index.
    pub delta_powers: BTreeMap<usize, i32>,
    pub period_powers: BTreeMap<u32, i32>,
}

impl SymMonomialSplit {
    pub fn pure(num_n: Poly, num_zw: ZwHandle, den_zw: ZwHandle) -> Self {
        Self {
            num_n,
            num_zw,
            den_zw,
            pi_power: 0,
            i_power: 0,
            log_powers: BTreeMap::new(),
            delta_powers: BTreeMap::new(),
            period_powers: BTreeMap::new(),
        }
    }
}

/// Bin a wide numerator by its W-side exponent tuple.
///
/// The denominator is interned once and shared by every returned leaf.  Terms
/// are transferred through Symbolica's coefficient/exponent arrays, so this
/// path performs no expression formatting or reparsing.
pub fn split_rat_by_w_monomial(
    rational: &Rat,
    narrow: Arc<PolyCtx>,
    table: &mut ZwTable,
    maps: &FnIndexMaps,
) -> Result<Vec<SymMonomialSplit>> {
    let wide = rational.ctx();
    if !table.ctx().is_compatible_with(wide) || maps.wide_to_narrow.len() != wide.len() {
        return Err(Error::ContextMismatch);
    }
    if maps.narrow_to_wide.len() != narrow.len() {
        return Err(Error::InvalidInput(
            "narrow-to-wide index map length mismatch".into(),
        ));
    }
    let wide_variables = wide.variable_map();
    let narrow_variables = narrow.variable_map();
    for (narrow_index, &wide_index) in maps.narrow_to_wide.iter().enumerate() {
        if wide_index >= wide.len()
            || maps.wide_to_narrow[wide_index] != Some(narrow_index)
            || wide_variables[wide_index] != narrow_variables[narrow_index]
        {
            return Err(Error::InvalidInput(
                "inconsistent wide/narrow variable index maps".into(),
            ));
        }
    }

    let denominator = table.intern(rational.denominator().clone(), ZwIntent::Denominator)?;
    if rational.is_zero() {
        return Ok(Vec::new());
    }

    let w_indices = maps
        .wide_to_narrow
        .iter()
        .enumerate()
        .filter_map(|(index, narrow_index)| narrow_index.is_none().then_some(index))
        .collect::<Vec<_>>();
    // Symbolica already implements this exact projection: selected variable
    // exponents become map keys and all remaining variables stay in the
    // coefficient polynomial.  Sorting the returned keys restores the stable
    // order required by HyperFLINT's transport layer.
    let mut bins = rational
        .numerator()
        .inner()
        .to_multivariate_polynomial_list(&w_indices, true)
        .into_iter()
        .map(|(exponents, coefficient)| (exponents.to_vec(), coefficient))
        .collect::<Vec<_>>();
    bins.sort_unstable_by(|left, right| left.0.cmp(&right.0));

    let mut leaves = Vec::with_capacity(bins.len());
    for (w_exponents, narrow_terms_in_wide) in bins {
        let num_n = Poly::from_inner(wide.clone(), narrow_terms_in_wide)
            .transplant(narrow.clone(), &maps.wide_to_narrow)?;
        let w_monomial = Poly::from_inner(
            wide.clone(),
            rational.numerator().inner().monomial(Q.one(), w_exponents),
        );
        let num_zw = table.intern(w_monomial, ZwIntent::Numerator)?;
        leaves.push(SymMonomialSplit::pure(num_n, num_zw, denominator));
    }
    Ok(leaves)
}

/// Recombine split leaves into a canonical rational function.
///
/// Unlike the first upstream Phase-A implementation, this supports leaves
/// with different denominator handles by adding their rational contributions.
pub fn recombine_rat_split(
    parts: &[SymMonomialSplit],
    table: &ZwTable,
    maps: &FnIndexMaps,
) -> Result<Rat> {
    let wide = table.ctx().clone();
    if parts.is_empty() {
        return Ok(Rat::zero(wide));
    }
    let source_to_destination = maps
        .narrow_to_wide
        .iter()
        .copied()
        .map(Some)
        .collect::<Vec<_>>();
    let mut result = Rat::zero(wide.clone());
    for part in parts {
        if part.num_n.ctx().len() != source_to_destination.len() {
            return Err(Error::InvalidInput(
                "split leaf uses an unexpected narrow context".into(),
            ));
        }
        let narrow_in_wide = part
            .num_n
            .transplant(wide.clone(), &source_to_destination)?;
        let numerator = narrow_in_wide.try_mul(table.get(part.num_zw)?)?;
        let contribution = Rat::new(numerator, table.get(part.den_zw)?.clone())?;
        result = result.try_add(&contribution)?;
    }
    Ok(result)
}

/// Narrow rational functions use the same canonical representation as `Rat`;
/// this alias records the narrower context contract in APIs.
pub type RatScalar = Rat;

#[cfg(test)]
mod tests {
    use symbolica::prelude::Symbol;

    use super::*;

    #[test]
    fn mixed_wide_numerator_round_trips_exactly() {
        let wide = PolyCtx::new(["x", "y", "s", "t"]).unwrap();
        let narrow = PolyCtx::new(["x", "y"]).unwrap();
        let maps = build_fn_index_maps(&wide, &narrow).unwrap();
        let source = Rat::parse(wide.clone(), "(s*x^2+t*x*y+s*y+1)/(s+x+1)").unwrap();
        let mut table = ZwTable::new(wide);
        let leaves = split_rat_by_w_monomial(&source, narrow, &mut table, &maps).unwrap();
        assert_eq!(leaves.len(), 3);
        assert!(leaves.iter().all(|leaf| leaf.den_zw == leaves[0].den_zw));
        assert_eq!(recombine_rat_split(&leaves, &table, &maps).unwrap(), source);
    }

    #[test]
    fn zero_uses_an_empty_leaf_list() {
        let wide = PolyCtx::new(["x", "s"]).unwrap();
        let narrow = PolyCtx::new(["x"]).unwrap();
        let maps = build_fn_index_maps(&wide, &narrow).unwrap();
        let mut table = ZwTable::new(wide.clone());
        let leaves = split_rat_by_w_monomial(&Rat::zero(wide), narrow, &mut table, &maps).unwrap();
        assert!(leaves.is_empty());
        assert!(
            recombine_rat_split(&leaves, &table, &maps)
                .unwrap()
                .is_zero()
        );
    }

    #[test]
    fn narrow_variables_must_be_a_wide_subset() {
        let wide = PolyCtx::new(["x", "s"]).unwrap();
        let invalid = PolyCtx::new(["x", "missing"]).unwrap();
        assert!(build_fn_index_maps(&wide, &invalid).is_err());
    }

    #[test]
    fn narrow_mapping_uses_symbolica_identity_not_stripped_names() {
        let wide_symbol = Symbol::parse("x", "split_wide").unwrap();
        let narrow_symbol = Symbol::parse("x", "split_narrow").unwrap();
        let wide = PolyCtx::from_indeterminates([wide_symbol.to_atom()]).unwrap();
        let narrow = PolyCtx::from_indeterminates([narrow_symbol.to_atom()]).unwrap();
        assert_eq!(wide.vars(), narrow.vars());
        assert!(build_fn_index_maps(&wide, &narrow).is_err());
    }

    #[test]
    fn narrow_mapping_allows_distinct_variables_with_the_same_display_name() {
        let left = Symbol::parse("x", "split_collision_left").unwrap();
        let right = Symbol::parse("x", "split_collision_right").unwrap();
        let wide = PolyCtx::from_indeterminates([left.to_atom(), right.to_atom()]).unwrap();
        let narrow = PolyCtx::from_indeterminates([right.to_atom(), left.to_atom()]).unwrap();
        assert_eq!(wide.vars(), &["x", "x"]);
        assert_eq!(narrow.vars(), &["x", "x"]);

        let maps = build_fn_index_maps(&wide, &narrow).unwrap();
        assert_eq!(maps.narrow_to_wide, vec![1, 0]);
        assert_eq!(maps.wide_to_narrow, vec![Some(1), Some(0)]);
    }
}
