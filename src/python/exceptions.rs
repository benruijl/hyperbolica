use pyo3::{
    Bound, PyErr, PyResult, create_exception,
    exceptions::PyException,
    types::{PyModule, PyModuleMethods},
};

use crate::{api::AtomIntegrationError, error::Error};

create_exception!(
    hyperbolica,
    HyperbolicaError,
    PyException,
    "Base class for all errors raised by Hyperbolica."
);
create_exception!(
    hyperbolica,
    InputError,
    HyperbolicaError,
    "The supplied expression, variables, or options are invalid."
);
create_exception!(
    hyperbolica,
    DuplicateVariableError,
    InputError,
    "An integration variable occurs more than once."
);
create_exception!(
    hyperbolica,
    AlgebraError,
    HyperbolicaError,
    "An exact algebra operation failed."
);
create_exception!(
    hyperbolica,
    DivergentIntegralError,
    HyperbolicaError,
    "The integral has a non-cancelling endpoint divergence."
);
create_exception!(
    hyperbolica,
    UnsupportedFeatureError,
    HyperbolicaError,
    "The requested integration feature is not available for this input."
);
create_exception!(
    hyperbolica,
    ContextError,
    AlgebraError,
    "The result refers to a symbol outside its exact polynomial context."
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExceptionKind {
    Input,
    DuplicateVariable,
    Algebra,
    Divergent,
    Context,
}

fn algebra_kind(error: &Error) -> ExceptionKind {
    match error {
        Error::PolynomialParse { .. }
        | Error::RationalParse { .. }
        | Error::UnknownVariable(_)
        | Error::InvalidExponent(_)
        | Error::InvalidInput(_) => ExceptionKind::Input,
        Error::ContextMismatch
        | Error::DivisionByZero
        | Error::InexactDivision
        | Error::Json(_) => ExceptionKind::Algebra,
    }
}

fn exception_kind(error: &AtomIntegrationError) -> ExceptionKind {
    match error {
        AtomIntegrationError::Algebra(error) => algebra_kind(error),
        AtomIntegrationError::DuplicateIntegrationVariable { .. } => {
            ExceptionKind::DuplicateVariable
        }
        AtomIntegrationError::Divergent { .. } => ExceptionKind::Divergent,
        AtomIntegrationError::SymbolicVariableOutsideContext { .. } => ExceptionKind::Context,
    }
}

pub(crate) fn integration_error(error: AtomIntegrationError) -> PyErr {
    let kind = exception_kind(&error);
    let message = error.to_string();
    match kind {
        ExceptionKind::Input => InputError::new_err(message),
        ExceptionKind::DuplicateVariable => DuplicateVariableError::new_err(message),
        ExceptionKind::Algebra => AlgebraError::new_err(message),
        ExceptionKind::Divergent => DivergentIntegralError::new_err(message),
        ExceptionKind::Context => ContextError::new_err(message),
    }
}

pub(crate) fn invalid_variable(index: usize, expression: &str) -> PyErr {
    InputError::new_err(format!(
        "integration variable at index {index} must be a plain Symbolica symbol, got `{expression}`"
    ))
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = module.py();
    module.add("HyperbolicaError", py.get_type::<HyperbolicaError>())?;
    module.add("InputError", py.get_type::<InputError>())?;
    module.add(
        "DuplicateVariableError",
        py.get_type::<DuplicateVariableError>(),
    )?;
    module.add("AlgebraError", py.get_type::<AlgebraError>())?;
    module.add(
        "DivergentIntegralError",
        py.get_type::<DivergentIntegralError>(),
    )?;
    module.add(
        "UnsupportedFeatureError",
        py.get_type::<UnsupportedFeatureError>(),
    )?;
    module.add("ContextError", py.get_type::<ContextError>())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_errors_have_stable_python_categories() {
        assert_eq!(
            exception_kind(&AtomIntegrationError::DuplicateIntegrationVariable {
                variable: "x".into(),
            }),
            ExceptionKind::DuplicateVariable
        );
        assert_eq!(
            exception_kind(&AtomIntegrationError::Algebra(Error::DivisionByZero)),
            ExceptionKind::Algebra
        );
        assert_eq!(
            exception_kind(&AtomIntegrationError::Algebra(Error::InvalidInput(
                "bad input".into(),
            ))),
            ExceptionKind::Input
        );
    }
}
