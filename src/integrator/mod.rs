//! Hyperlogarithm word regularization and integration-variable transforms.

pub mod factor_table;
pub mod lr_scan;
pub mod lr_search;
pub mod lr_verify;

mod differentiate;
mod hyper_int;
mod integration_step;
mod lr_find_roots;
mod lr_reduction;
mod primitive;
mod regularize;
mod structural_keys;
mod transform;

pub use differentiate::differentiate_wordlist;
pub(crate) use hyper_int::rescale_interval_with_bound_parser;
pub use hyper_int::{
    HyperIntOptions, hyper_int, hyper_int_with_options, hyper_int_with_options_and_spectators,
    hyperflint, hyperflint_sym, hyperflint_with_options, hyperflint_with_options_and_spectators,
    rescale_interval,
};
pub use integration_step::{
    Boundary, IntegrationError, IntegrationResult, IntegrationStepOptions, ShuffleEntry,
    ShuffleEntrySym, ShuffleList, ShuffleListSym, close_positive_letters, integration_step,
    integration_step_sym, integration_step_sym_with_options,
    integration_step_sym_with_options_and_remaining_variables, integration_step_with_options,
    integration_step_with_options_and_remaining_variables,
};
pub use lr_verify::{OrderVerifyResult, verify_order_is_lr};
pub(crate) use primitive::integrate_ii_with_factored_prefactor;
pub use primitive::{IntegrateIiOptions, integrate_ii, integrate_ii_with_options};
pub use regularize::{reg_head, reg_tail, reg0, regzero_word, regzero_word_in_ctx};
pub(crate) use transform::regkey_structural_cmp;
pub(crate) use transform::transform_shuffle_with_options_and_table;
pub(crate) use transform::transform_word_with_options_and_table;
pub use transform::{
    RegKey, RegTerm, RegTermSym, Regulator, RegulatorSym, TransformOptions, TransformPair,
    TransformResult, canonicalize_regkey, canonicalize_regulator, canonicalize_regulator_sym,
    collect_regulator, collect_regulator_sym, regkey_content_key, reglim_word,
    reglim_word_with_table, regulator_content_key, regulator_sym_content_key, shuffle_symbolic,
    shuffle_symbolic_sym, transform_shuffle, transform_shuffle_with_options, transform_word,
    transform_word_with_options,
};
