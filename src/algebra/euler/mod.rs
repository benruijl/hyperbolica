//! Pure-Symbolica Euler-characteristic filtering for LR subset tables.
//!
//! The implementation follows the cleared-logarithmic-derivative construction
//! used by HyperFLINT, but keeps every algebraic operation in-process and in
//! Symbolica's typed polynomial domains.  No external solver or CAS protocol is
//! involved.

mod filter;
mod random;
mod staircase;
mod system;

pub use filter::{
    ChiFilterCache, ChiFilterStats, chi_filter_letters, chi_filter_stats, chi_letter_genuine,
    reset_chi_filter_stats,
};
pub use staircase::{ChiCount, ChiStatus, chi_staircase_count};
pub use system::{
    ChiSystemTimings, chi_count_sectors, chi_system_timings, reset_chi_system_timings,
};
