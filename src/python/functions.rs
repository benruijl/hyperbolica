use pyo3::{
    Bound, PyResult, Python, pyfunction,
    types::{PyModule, PyModuleMethods},
    wrap_pyfunction,
};
use symbolica::api::python::PythonExpression;

use crate::api::{AtomIntegrationOptions, integrate_atom, prepare_atom_with_options};

use super::{
    exceptions, input::integration_symbols, options::PythonIntegrationOptions,
    prepared::PythonPreparedIntegral, result::PythonIntegrationResult,
};

fn options_or_default(options: Option<&PythonIntegrationOptions>) -> AtomIntegrationOptions {
    options.map_or_else(AtomIntegrationOptions::default, |value| value.to_rust())
}

fn run_integration(
    py: Python<'_>,
    expression: &PythonExpression,
    variables: &[PythonExpression],
    options: Option<&PythonIntegrationOptions>,
) -> PyResult<PythonIntegrationResult> {
    let input = expression.expr.clone();
    let variables = integration_symbols(variables)?;
    let options = options_or_default(options);
    py.detach(move || integrate_atom(&input, &variables, &options))
        .map(PythonIntegrationResult::new)
        .map_err(exceptions::integration_error)
}

/// Integrate a Symbolica expression over `[0, infinity)` in variable order.
///
/// Parameters
/// ----------
/// expression:
///     A native `symbolica.Expression`. It is never converted to text.
/// variables:
///     Plain Symbolica symbols in integration order.
/// options:
///     Optional `IntegrationOptions`.
///
/// Returns
/// -------
/// symbolica.Expression
///     The normalized exact result.
#[pyfunction(signature = (expression, variables, options = None))]
fn integrate(
    py: Python<'_>,
    expression: &PythonExpression,
    variables: Vec<PythonExpression>,
    options: Option<&PythonIntegrationOptions>,
) -> PyResult<PythonExpression> {
    run_integration(py, expression, &variables, options)?.into_expression()
}

/// Integrate and return an inspectable `IntegrationResult`.
#[pyfunction(signature = (expression, variables, options = None))]
fn integrate_detailed(
    py: Python<'_>,
    expression: &PythonExpression,
    variables: Vec<PythonExpression>,
    options: Option<&PythonIntegrationOptions>,
) -> PyResult<PythonIntegrationResult> {
    run_integration(py, expression, &variables, options)
}

/// Lower a Symbolica expression once for repeated integrations or option
/// sweeps. Mathematical input remains an Atom throughout preparation.
#[pyfunction(signature = (expression, variables, options = None))]
fn prepare(
    py: Python<'_>,
    expression: &PythonExpression,
    variables: Vec<PythonExpression>,
    options: Option<&PythonIntegrationOptions>,
) -> PyResult<PythonPreparedIntegral> {
    let input = expression.expr.clone();
    let variables = integration_symbols(&variables)?;
    let options = options_or_default(options);
    let preparation_options = options.clone();
    py.detach(move || prepare_atom_with_options(&input, &variables, &options))
        .map(|prepared| PythonPreparedIntegral::new(prepared, preparation_options))
        .map_err(exceptions::integration_error)
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(integrate, module)?)?;
    module.add_function(wrap_pyfunction!(integrate_detailed, module)?)?;
    module.add_function(wrap_pyfunction!(prepare, module)?)?;
    Ok(())
}
