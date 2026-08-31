use thiserror::Error;

/// Errors produced by the exact algebra and HyperFLINT compatibility layer.
#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid polynomial expression `{expression}`: {message}")]
    PolynomialParse { expression: String, message: String },

    #[error("invalid rational expression `{expression}`: {message}")]
    RationalParse { expression: String, message: String },

    #[error("unknown variable `{0}`")]
    UnknownVariable(String),

    #[error("polynomial contexts differ")]
    ContextMismatch,

    #[error("division by zero")]
    DivisionByZero,

    #[error("polynomial division is not exact")]
    InexactDivision,

    #[error("invalid exponent {0}")]
    InvalidExponent(i64),

    #[error("{0}")]
    InvalidInput(String),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
