//! Atom-native, typed public API.
//!
//! The compatibility bridge accepts strings and JSON.  This module is the
//! production boundary for Rust callers and the planned PyO3 binding: it
//! accepts Symbolica [`Atom`] values directly, exposes
//! a reusable prepared representation, and returns structured terms that can
//! be materialized back into a normalized Atom.

pub use symbolica::prelude::{Atom, AtomCore, AtomView, Symbol};

mod error;
mod input;
mod integrate;
mod options;
mod output;

pub use error::{AtomIntegrationError, AtomIntegrationResult};
pub use input::{PreparedAtomInput, prepare_atom, prepare_atom_with_options};
pub use integrate::{
    integrate_atom, integrate_atom_over, integrate_prepared_atom, integrate_prepared_atom_over,
    prepare_atom_over,
};
pub use options::{AtomIntegrationOptions, IntegrationEndpoint, IntegrationInterval};
pub use output::{
    AtomIntegrationOutput, AtomIntegrationTerm, period_word_to_atom, symcoef_to_atom,
};
