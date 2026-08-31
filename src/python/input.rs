use pyo3::PyResult;
use symbolica::{
    api::python::PythonExpression,
    atom::{AtomView, Symbol},
};

use super::exceptions;

pub(crate) fn integration_symbols(variables: &[PythonExpression]) -> PyResult<Vec<Symbol>> {
    variables
        .iter()
        .enumerate()
        .map(|(index, expression)| match expression.expr.as_view() {
            AtomView::Var(variable) => Ok(variable.get_symbol()),
            _ => Err(exceptions::invalid_variable(
                index,
                &expression.expr.to_string(),
            )),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use symbolica::prelude::{Atom, symbol};

    use super::*;

    #[test]
    fn variable_expressions_are_extracted_without_parsing() {
        let (x, y) = symbol!("python_input_x", "python_input_y");
        let variables = vec![x.to_atom().into(), y.to_atom().into()];
        assert_eq!(integration_symbols(&variables).unwrap(), vec![x, y]);
    }

    #[test]
    fn only_plain_symbols_are_accepted() {
        let x = symbol!("python_input_not_plain_x");
        let variables = vec![PythonExpression::from(x + Atom::one())];
        assert!(integration_symbols(&variables).is_err());
    }
}
