mod algebraic_introduction;
pub mod algebraic_letters;
pub mod convert;
pub mod diff;
pub mod euler;
pub mod linear_factors;
pub mod partial_fractions;
pub mod shuffle;

pub use algebraic_letters::{
    AlgebraicLetterEntry, AlgebraicLetterSession, AlgebraicLetterTable,
    DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE, algebraic_letters_allocate, algebraic_letters_clear,
    algebraic_letters_show, algebraic_letters_size, back_substitute,
    begin_algebraic_letter_session, build_algebraic_letter_atom_list, build_full_atom_list,
    combine_wm_wp_ratios, simplify_with_vieta,
};
pub use linear_factors::{
    LinearFactor, LinearFactorOptions, LinearFactorization, NonlinearFactor, linear_factors,
    linear_factors_with_options,
};
pub use partial_fractions::{
    PartialFractionOptions, PartialFractionPole, PartialFractionization, partial_fractions,
    partial_fractions_factored, partial_fractions_factored_with_options,
    partial_fractions_with_options,
};
