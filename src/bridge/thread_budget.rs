//! Request-scoped compatibility for `HF_MAX_THREADS_PER_CALL`.
//!
//! The pinned C++ handler reads this variable only at `hyperflint_sym` entry
//! and passes a positive `atoi` result to FLINT's process-global thread
//! setter.  Mutating Rayon's global pool per request would race when the C ABI
//! or LibraryLink adapter is called concurrently.  Instead, positive limits
//! greater than one get an isolated local pool.  A limit of one stays on the
//! calling thread and asks the bridge to disable its coarse parallel path;
//! this also preserves Symbolica's restricted-mode calling-thread contract.

use std::ffi::OsStr;

use rayon::ThreadPoolBuilder;

use crate::error::{Error, Result};

const ENV_NAME: &str = "HF_MAX_THREADS_PER_CALL";

/// Run one full-integration request under the environment-selected budget.
///
/// The boolean passed to `operation` requests the explicit serial path.  It is
/// used only for an effective one-thread limit; larger limits are enforced by
/// the local Rayon pool in which `operation` runs.
pub(super) fn with_max_threads_per_call<T, F>(operation: F) -> Result<T>
where
    T: Send,
    F: FnOnce(bool) -> Result<T> + Send,
{
    if cfg!(target_arch = "wasm32") {
        return operation(true);
    }
    let Some(requested) = std::env::var_os(ENV_NAME)
        .as_deref()
        .and_then(parse_positive_atoi)
    else {
        return operation(false);
    };

    let available = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    // `HF_MAX_THREADS_PER_CALL` is a ceiling. Avoid oversubscribing the CPU
    // allocation, and do not create more Symbolica-calling workers than the
    // active license permits. Checking the license is unnecessary for the
    // common one-thread memory-saving setting.
    let effective = requested.min(available);
    let effective = if effective == 1 {
        1
    } else {
        symbolica::license::LicenseManager::max_threads(effective)
    };

    with_effective_thread_limit(effective, operation)
}

pub(super) fn with_effective_thread_limit<T, F>(effective: usize, operation: F) -> Result<T>
where
    T: Send,
    F: FnOnce(bool) -> Result<T> + Send,
{
    if cfg!(target_arch = "wasm32") || effective <= 1 {
        return operation(true);
    }

    let pool = ThreadPoolBuilder::new()
        .num_threads(effective)
        .thread_name(|index| format!("hyperbolica-call-{index}"))
        .build()
        .map_err(|error| {
            Error::InvalidInput(format!("could not apply {ENV_NAME}={effective}: {error}"))
        })?;
    pool.install(|| operation(false))
}

/// Parse the defined, useful portion of C's `atoi` contract.
///
/// Leading ASCII whitespace and one sign are accepted, parsing stops at the
/// first non-digit, and zero/negative/no-digit values mean "no override".
/// Values outside positive `int` range are ignored because the corresponding
/// C++ `atoi` behavior is undefined.
fn parse_positive_atoi(value: &OsStr) -> Option<usize> {
    let bytes = value.as_encoded_bytes();
    let mut index = bytes
        .iter()
        .position(|byte| !is_c_ascii_whitespace(*byte))?;
    let negative = match bytes[index] {
        b'+' => {
            index += 1;
            false
        }
        b'-' => {
            index += 1;
            true
        }
        _ => false,
    };

    let digit_start = index;
    let mut magnitude = 0_u32;
    while let Some(&byte) = bytes.get(index) {
        if !byte.is_ascii_digit() {
            break;
        }
        magnitude = magnitude
            .checked_mul(10)?
            .checked_add(u32::from(byte - b'0'))?;
        index += 1;
    }
    if index == digit_start || negative || magnitude == 0 || magnitude > i32::MAX as u32 {
        return None;
    }
    Some(magnitude as usize)
}

const fn is_c_ascii_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::process::Command;
    use std::sync::{Arc, Barrier};
    use std::thread;

    use super::*;

    #[test]
    fn parser_matches_upstream_positive_atoi_semantics() {
        for (input, expected) in [
            ("", None),
            ("   ", None),
            ("\u{b}1", Some(1)),
            ("\u{c}2workers", Some(2)),
            ("garbage", None),
            ("+", None),
            ("0", None),
            ("-7", None),
            ("  +003workers", Some(3)),
            ("17.5", Some(17)),
            ("1e3", Some(1)),
            ("2147483647", Some(i32::MAX as usize)),
            ("2147483648", None),
        ] {
            assert_eq!(
                parse_positive_atoi(OsStr::new(input)),
                expected,
                "{input:?}"
            );
        }
    }

    #[test]
    fn one_thread_limit_stays_on_the_calling_thread_and_requests_serial_work() {
        let caller = thread::current().id();
        let observed = with_effective_thread_limit(1, |force_serial| {
            Ok((thread::current().id(), force_serial))
        })
        .unwrap();
        assert_eq!(observed, (caller, true));
    }

    #[test]
    fn environment_limit_is_exercised_in_an_isolated_process() {
        const PROBE: &str = "HYPERBOLICA_THREAD_BUDGET_TEST_PROBE";
        if std::env::var_os(PROBE).is_some() {
            let caller = thread::current().id();
            let observed = with_max_threads_per_call(|force_serial| {
                Ok((thread::current().id(), force_serial))
            })
            .unwrap();
            assert_eq!(observed, (caller, true));
            return;
        }

        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "bridge::thread_budget::tests::environment_limit_is_exercised_in_an_isolated_process",
            ])
            .env(PROBE, "1")
            .env(ENV_NAME, "  +1worker")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child stdout:\n{}\nchild stderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn concurrent_calls_use_independent_request_scoped_pools() {
        let rendezvous = Arc::new(Barrier::new(3));
        let first_barrier = rendezvous.clone();
        let first = thread::spawn(move || {
            with_effective_thread_limit(2, |force_serial| {
                first_barrier.wait();
                Ok((rayon::current_num_threads(), force_serial))
            })
            .unwrap()
        });
        let second_barrier = rendezvous.clone();
        let second = thread::spawn(move || {
            with_effective_thread_limit(3, |force_serial| {
                second_barrier.wait();
                Ok((rayon::current_num_threads(), force_serial))
            })
            .unwrap()
        });
        rendezvous.wait();

        assert_eq!(first.join().unwrap(), (2, false));
        assert_eq!(second.join().unwrap(), (3, false));
    }
}
