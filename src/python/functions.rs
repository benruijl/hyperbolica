use pyo3::{
    Bound, PyResult, Python, pyfunction,
    types::{PyModule, PyModuleMethods},
    wrap_pyfunction,
};
use symbolica::api::python::PythonExpression;

use crate::api::{
    AtomIntegrationOptions, IntegrationEndpoint, IntegrationInterval, integrate_atom,
    integrate_atom_over, prepare_atom_with_options,
};

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
        .map_err(|error| exceptions::integration_error(py, error))
}

fn native_intervals(
    py: Python<'_>,
    intervals: Vec<(PythonExpression, PythonExpression)>,
) -> PyResult<Vec<IntegrationInterval>> {
    intervals
        .into_iter()
        .map(|(from, to)| {
            let from = IntegrationEndpoint::try_from_atom(from.expr)
                .map_err(|error| exceptions::integration_error(py, error.into()))?;
            let to = IntegrationEndpoint::try_from_atom(to.expr)
                .map_err(|error| exceptions::integration_error(py, error.into()))?;
            Ok(IntegrationInterval::new(from, to))
        })
        .collect()
}

fn run_integration_over(
    py: Python<'_>,
    expression: &PythonExpression,
    variables: &[PythonExpression],
    intervals: Vec<(PythonExpression, PythonExpression)>,
    options: Option<&PythonIntegrationOptions>,
) -> PyResult<PythonIntegrationResult> {
    let input = expression.expr.clone();
    let variables = integration_symbols(variables)?;
    let intervals = native_intervals(py, intervals)?;
    let options = options_or_default(options);
    py.detach(move || integrate_atom_over(&input, &variables, &intervals, &options))
        .map(PythonIntegrationResult::new)
        .map_err(|error| exceptions::integration_error(py, error))
}

/// Integrate a Symbolica expression over `[0, infinity)` in variable order.
///
/// Parameters
/// ----------
/// expression:
///     A native `hyperbolica.Expression` from the shipped Symbolica kernel.
///     It is never converted to text.
/// variables:
///     Plain Symbolica symbols in integration order.
/// options:
///     Optional `IntegrationOptions`.
///
/// Returns
/// -------
/// hyperbolica.Expression
///     The normalized exact result.
#[pyfunction(
    signature = (expression, variables, options = None),
    text_signature = "(expression, variables, options=None)"
)]
fn integrate(
    py: Python<'_>,
    expression: &PythonExpression,
    variables: Vec<PythonExpression>,
    options: Option<&PythonIntegrationOptions>,
) -> PyResult<PythonExpression> {
    run_integration(py, expression, &variables, options)?.into_expression(py)
}

/// Integrate over arbitrary directed intervals in variable order.
///
/// Parameters
/// ----------
/// expression:
///     A native `hyperbolica.Expression` from the shipped Symbolica kernel.
/// variables:
///     Plain Symbolica symbols in integration order.
/// intervals:
///     One `(from, to)` pair of native expressions per variable. Use
///     `hyperbolica.Symbol.INFINITY` (or its negation) for directed infinity.
/// options:
///     Optional `IntegrationOptions`.
///
/// Returns
/// -------
/// hyperbolica.Expression
///     The normalized exact result.
#[pyfunction(
    signature = (expression, variables, intervals, options = None),
    text_signature = "(expression, variables, intervals, options=None)"
)]
fn integrate_over(
    py: Python<'_>,
    expression: &PythonExpression,
    variables: Vec<PythonExpression>,
    intervals: Vec<(PythonExpression, PythonExpression)>,
    options: Option<&PythonIntegrationOptions>,
) -> PyResult<PythonExpression> {
    run_integration_over(py, expression, &variables, intervals, options)?.into_expression(py)
}

/// Integrate and return an inspectable exact result.
///
/// Parameters
/// ----------
/// expression:
///     A native `hyperbolica.Expression` from the shipped Symbolica kernel.
/// variables:
///     Plain shipped Symbolica symbols in integration order.
/// options:
///     Optional `IntegrationOptions`.
///
/// Returns
/// -------
/// IntegrationResult
///     The returned
/// `IntegrationResult` retains variables, indeterminates, algebraic-letter
/// metadata, and the collected term count in addition to its expression.
#[pyfunction(
    signature = (expression, variables, options = None),
    text_signature = "(expression, variables, options=None)"
)]
fn integrate_detailed(
    py: Python<'_>,
    expression: &PythonExpression,
    variables: Vec<PythonExpression>,
    options: Option<&PythonIntegrationOptions>,
) -> PyResult<PythonIntegrationResult> {
    run_integration(py, expression, &variables, options)
}

/// Integrate over arbitrary directed intervals and retain exact metadata.
///
/// `intervals` contains one `(from, to)` pair per integration variable and
/// accepts native real directed infinities through `Symbol.INFINITY`.
#[pyfunction(
    signature = (expression, variables, intervals, options = None),
    text_signature = "(expression, variables, intervals, options=None)"
)]
fn integrate_detailed_over(
    py: Python<'_>,
    expression: &PythonExpression,
    variables: Vec<PythonExpression>,
    intervals: Vec<(PythonExpression, PythonExpression)>,
    options: Option<&PythonIntegrationOptions>,
) -> PyResult<PythonIntegrationResult> {
    run_integration_over(py, expression, &variables, intervals, options)
}

/// Lower a Symbolica expression once for repeated integrations or option sweeps.
///
/// The returned immutable `PreparedIntegral` owns the lowered input and an
/// independent copy of `options`. Mathematical input remains a native
/// Symbolica expression throughout preparation.
///
/// Parameters
/// ----------
/// expression:
///     A native `hyperbolica.Expression` from the shipped Symbolica kernel.
/// variables:
///     Plain shipped Symbolica symbols in integration order.
/// options:
///     Options captured as the prepared object's default integration options.
///
/// Returns
/// -------
/// PreparedIntegral
///     Reusable immutable lowered input.
#[pyfunction(
    signature = (expression, variables, options = None),
    text_signature = "(expression, variables, options=None)"
)]
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
        .map_err(|error| exceptions::integration_error(py, error))
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(integrate, module)?)?;
    module.add_function(wrap_pyfunction!(integrate_over, module)?)?;
    module.add_function(wrap_pyfunction!(integrate_detailed, module)?)?;
    module.add_function(wrap_pyfunction!(integrate_detailed_over, module)?)?;
    module.add_function(wrap_pyfunction!(prepare, module)?)?;
    Ok(())
}
