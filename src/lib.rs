//! Hyperlogarithm integration and linear-reducibility search backed by Symbolica.
//!
//! This crate is a Rust port of SubTropica's HyperFLINT.  It deliberately keeps
//! the mathematical vocabulary and JSON protocol of the original while using
//! Symbolica for exact polynomial and rational-function arithmetic.

pub mod algebra;
pub mod api;
pub mod bridge;
pub mod c_abi;
pub mod convert;
pub mod core;
pub mod error;
pub mod integrator;
#[cfg(feature = "python")]
pub mod python;
pub mod reduce;
pub mod series;
pub mod symbols;

pub use error::{Error, Result};

/// Convenient imports for the Atom-native Rust API.
pub mod prelude {
    pub use symbolica::prelude::{Atom, AtomCore, AtomView, Symbol, symbol};

    pub use crate::api::{
        AtomIntegrationError, AtomIntegrationOptions, AtomIntegrationOutput, AtomIntegrationResult,
        AtomIntegrationTerm, PreparedAtomInput, integrate_atom, integrate_prepared_atom,
        prepare_atom, prepare_atom_with_options,
    };
    pub use crate::symbols::{HyperbolicaSymbols, heads};
}
