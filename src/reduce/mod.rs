//! MZV reductions, period evaluation, and contour deformation.

mod break_up_contour;
mod mzv_expansion;
mod mzv_reduce;
mod periods;

pub use break_up_contour::{
    OnAxisEntry, OnAxisSymEntry, WordlistSym, WordlistSymTerm, break_up_contour,
    break_up_contour_sym, to_wordlist_sym,
};
pub use mzv_expansion::{
    MzvExpansionTable, assert_no_lhs_tokens, build_basis_atom_list, build_basis_var_list,
    cross_ctx_transfer_rat, expand_mzv_reductions, load_mzv_expansion,
    load_mzv_expansion_with_options, looks_like_mzv, standard_mzv_expansion, tokens_in,
    weight_of_mzv_name,
};
pub use mzv_reduce::{
    MzvReductionRule, MzvReductionTable, apply_mzv_reductions, build_mzv_atom_list,
    build_mzv_basis_atom_list, build_mzv_var_list, build_narrow_var_list, indices_from_mzv_name,
    load_mzv_reductions, mzv_constant_atom, mzv_expression_atom, standard_mzv_reductions,
    substitute_var_rat,
};
pub use periods::{
    FibrationBasisResult, FibrationBasisResultSym, evaluate_periods, fibration_basis,
    fibration_basis_sym, test_zero_function, test_zero_function_sym, to_mzv, to_mzv_with_expansion,
    zero_inf_period, zero_inf_period_with_expansion, zero_one_period,
    zero_one_period_with_expansion,
};
