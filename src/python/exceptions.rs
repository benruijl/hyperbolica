use pyo3::{
    Bound, PyErr, PyResult, Python, create_exception,
    exceptions::PyException,
    types::{PyAnyMethods, PyModule, PyModuleMethods},
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

fn user_variable_name(variable: &str) -> &str {
    variable.rsplit("::").next().unwrap_or(variable)
}

pub(crate) fn integration_error(py: Python<'_>, error: AtomIntegrationError) -> PyErr {
    let kind = exception_kind(&error);
    let message = error.to_string();
    let python_error = match kind {
        ExceptionKind::Input => InputError::new_err(message),
        ExceptionKind::DuplicateVariable => DuplicateVariableError::new_err(message),
        ExceptionKind::Algebra => AlgebraError::new_err(message),
        ExceptionKind::Divergent => DivergentIntegralError::new_err(message),
        ExceptionKind::Context => ContextError::new_err(message),
    };

    // Preserve structured fields for callers that need programmatic error
    // handling while keeping the original human-readable message in args[0].
    let attributes = (|| -> PyResult<()> {
        match &error {
            AtomIntegrationError::DuplicateIntegrationVariable { variable } => {
                python_error
                    .value(py)
                    .setattr("variable", user_variable_name(variable))?;
            }
            AtomIntegrationError::Divergent {
                boundary,
                variable,
                log_power,
                power,
            } => {
                let value = python_error.value(py);
                value.setattr("boundary", boundary.to_string())?;
                value.setattr("variable", user_variable_name(variable))?;
                value.setattr("log_power", *log_power)?;
                value.setattr("power", *power)?;
            }
            AtomIntegrationError::SymbolicVariableOutsideContext { variable } => {
                python_error
                    .value(py)
                    .setattr("variable", user_variable_name(variable))?;
            }
            AtomIntegrationError::Algebra(_) => {}
        }
        Ok(())
    })();
    if let Err(attribute_error) = attributes {
        return attribute_error;
    }
    python_error
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
        assert_eq!(
            user_variable_name("hyperbolica::python_duplicate_x"),
            "python_duplicate_x"
        );
        assert_eq!(user_variable_name("plain_x"), "plain_x");
    }
}
