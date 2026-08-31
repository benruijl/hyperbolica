//! Construction of registered algebraic-letter and reduction indeterminates.

use symbolica::prelude::Atom;

use crate::error::Result;
use crate::reduce::{MzvReductionTable, build_mzv_atom_list};
use crate::symbols::algebraic_atoms;

/// Append a deterministic pool of registered algebraic indeterminates.
pub fn build_algebraic_letter_atom_list(
    variables: impl IntoIterator<Item = Atom>,
    pool_size: usize,
) -> Vec<Atom> {
    let mut output = Vec::new();
    let mut add = |atom: Atom| {
        if !output.contains(&atom) {
            output.push(atom);
        }
    };
    for variable in variables {
        add(variable);
    }
    for idx in 1..=pool_size {
        let atoms = algebraic_atoms(
            u32::try_from(idx).expect("algebraic-letter pool index must fit in u32"),
        );
        add(atoms.minus);
        add(atoms.plus);
        add(atoms.ratio);
        add(atoms.sqrt_discriminant);
    }
    output
}

/// Combine user, MZV, and algebraic-letter registered indeterminates.
pub fn build_full_atom_list(
    reductions: &MzvReductionTable,
    variables: impl IntoIterator<Item = Atom>,
    pool_size: usize,
) -> Result<Vec<Atom>> {
    Ok(build_algebraic_letter_atom_list(
        build_mzv_atom_list(reductions, variables)?,
        pool_size,
    ))
}
