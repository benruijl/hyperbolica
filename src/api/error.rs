use crate::error::Error;
use crate::integrator::{Boundary, IntegrationError};

/// Errors exposed by the Atom-native integration boundary.
///
/// The variants intentionally carry owned, transport-friendly data.  A future
/// PyO3 wrapper can therefore translate them into Python exceptions without
/// having to inspect internal error strings.
#[derive(Debug, thiserror::Error)]
pub enum AtomIntegrationError {
    #[error(transparent)]
    Algebra(#[from] Error),

    #[error("integration variable `{variable}` occurs more than once")]
    DuplicateIntegrationVariable { variable: String },

    #[error(
        "divergent integral at {boundary} for `{variable}`: \
         Log[{variable}]^{log_power} / {variable}^{power}"
    )]
    Divergent {
        boundary: Boundary,
        variable: String,
        log_power: i64,
        power: i64,
    },

    #[error("symbolic coefficient refers to `{variable}`, which is not in the Atom context")]
    SymbolicVariableOutsideContext { variable: String },
}

impl From<IntegrationError> for AtomIntegrationError {
    fn from(error: IntegrationError) -> Self {
        match error {
            IntegrationError::Algebra(error) => Self::Algebra(error),
            IntegrationError::Divergent {
                boundary,
                variable,
                log_power,
                power,
            } => Self::Divergent {
                boundary,
                variable,
                log_power,
                power,
            },
        }
    }
}

pub type AtomIntegrationResult<T> = std::result::Result<T, AtomIntegrationError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integration_errors_retain_structured_divergence_data() {
        let error = AtomIntegrationError::from(IntegrationError::Divergent {
            boundary: Boundary::Infinity,
            variable: "x".into(),
            log_power: 2,
            power: 1,
        });
        assert!(matches!(
            error,
            AtomIntegrationError::Divergent {
                boundary: Boundary::Infinity,
                log_power: 2,
                power: 1,
                ..
            }
        ));
    }
}
