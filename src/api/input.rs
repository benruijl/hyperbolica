use std::collections::HashSet;
use std::sync::Arc;

use symbolica::prelude::{Atom, AtomView, Symbol};

use crate::convert::{convert_to_hlog_reg_inf, expression_from_atom_with_indeterminates};
use crate::core::PolyCtx;
use crate::integrator::{ShuffleEntry, ShuffleList};

use super::{AtomIntegrationError, AtomIntegrationOptions, AtomIntegrationResult};

/// Atom input after native lowering and Hlog-at-infinity conversion.
///
/// Preparing once and integrating repeatedly is useful for option sweeps and
/// benchmarks: neither Symbolica-to-ring conversion nor shuffle conversion is
/// repeated.
#[derive(Clone, Debug)]
pub struct PreparedAtomInput {
    ctx: Arc<PolyCtx>,
    indeterminates: Vec<Atom>,
    integration_variables: Vec<Symbol>,
    integration_indices: Vec<usize>,
    shuffle_list: ShuffleList,
}

impl PreparedAtomInput {
    pub fn context(&self) -> &Arc<PolyCtx> {
        &self.ctx
    }

    pub fn indeterminates(&self) -> &[Atom] {
        &self.indeterminates
    }

    pub fn integration_variables(&self) -> &[Symbol] {
        &self.integration_variables
    }

    pub fn integration_indices(&self) -> &[usize] {
        &self.integration_indices
    }

    pub fn shuffle_list(&self) -> &ShuffleList {
        &self.shuffle_list
    }
}

fn validate_variables(variables: &[Symbol]) -> AtomIntegrationResult<()> {
    let mut seen = HashSet::with_capacity(variables.len());
    for variable in variables {
        if !seen.insert(*variable) {
            return Err(AtomIntegrationError::DuplicateIntegrationVariable {
                variable: variable.get_name().to_owned(),
            });
        }
    }
    Ok(())
}

/// Lower a concrete Symbolica Atom without string reparsing.
pub fn prepare_atom(
    input: &Atom,
    integration_variables: &[Symbol],
) -> AtomIntegrationResult<PreparedAtomInput> {
    prepare_atom_with_options(
        input,
        integration_variables,
        &AtomIntegrationOptions::default(),
    )
}

/// Lower a concrete Atom while reserving every exact constant the options may
/// introduce during later reductions.
pub fn prepare_atom_with_options(
    input: &Atom,
    integration_variables: &[Symbol],
    options: &AtomIntegrationOptions,
) -> AtomIntegrationResult<PreparedAtomInput> {
    prepare_atom_view(input.as_view(), integration_variables, options)
}

pub(crate) fn prepare_atom_view(
    input: AtomView<'_>,
    integration_variables: &[Symbol],
    options: &AtomIntegrationOptions,
) -> AtomIntegrationResult<PreparedAtomInput> {
    validate_variables(integration_variables)?;
    let reserved = options.reserved_indeterminates()?;
    let lowered =
        expression_from_atom_with_indeterminates(input, integration_variables, &reserved)?;
    let regulator = convert_to_hlog_reg_inf(&lowered.expr, &lowered.ctx)?;
    let shuffle_list = regulator
        .into_iter()
        .map(|term| ShuffleEntry::new(term.coef, term.key))
        .collect::<Vec<_>>();
    let integration_indices = integration_variables
        .iter()
        .map(|variable| {
            lowered
                .ctx
                .index_of_symbol(*variable)
                .ok_or_else(|| crate::error::Error::UnknownVariable(variable.get_name().to_owned()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(PreparedAtomInput {
        ctx: lowered.ctx,
        indeterminates: lowered.indeterminates,
        integration_variables: integration_variables.to_vec(),
        integration_indices,
        shuffle_list,
    })
}

#[cfg(test)]
mod tests {
    use symbolica::prelude::{Atom, AtomCore, symbol};

    use crate::symbols::heads;
    use crate::{
        reduce::{MzvReductionRule, MzvReductionTable},
        symbols::{log_two_atom, mzv_atom},
    };

    use super::*;

    #[test]
    fn accepts_borrowed_atom_and_preserves_requested_variable_order() {
        let (x, y) = symbol!("api_input_x", "api_input_y");
        let input = Atom::one() / (Atom::one() + x + y).pow(2);
        let prepared = prepare_atom(&input, &[y, x]).unwrap();
        assert_eq!(prepared.integration_indices(), &[0, 1]);
        assert_eq!(prepared.integration_variables(), &[y, x]);
        assert_eq!(prepared.indeterminates()[0], y.to_atom());
        assert_eq!(prepared.indeterminates()[1], x.to_atom());
        assert_eq!(prepared.shuffle_list().len(), 1);
    }

    #[test]
    fn accepts_atom_view_and_retains_hlog_words() {
        let x = symbol!("api_input_hlog_x");
        let input = heads().hlog.call((x, Atom::zero(), -1));
        let prepared = prepare_atom(&input, &[x]).unwrap();
        assert_eq!(prepared.shuffle_list().len(), 2);
        assert!(
            prepared
                .shuffle_list()
                .iter()
                .all(|entry| { entry.shuffle.len() == 1 && entry.shuffle[0].len() == 2 })
        );
    }

    #[test]
    fn registered_function_indeterminates_are_not_replaced_by_dynamic_symbols() {
        let x = symbol!("api_input_algebraic_x");
        let algebraic = heads().algebraic_minus.call(3);
        let input = Atom::one() / (x + &algebraic);
        let prepared = prepare_atom(&input, &[x]).unwrap();
        assert_eq!(prepared.indeterminates()[0], x.to_atom());
        assert!(
            prepared
                .indeterminates()
                .iter()
                .any(|candidate| candidate == &algebraic)
        );
        assert!(
            prepared
                .context()
                .variable_atom(1)
                .is_ok_and(|candidate| candidate == algebraic)
        );
    }

    #[test]
    fn duplicate_integration_variables_are_rejected_before_lowering() {
        let x = symbol!("api_input_duplicate_x");
        let error = prepare_atom(&Atom::one(), &[x, x]).unwrap_err();
        assert!(matches!(
            error,
            AtomIntegrationError::DuplicateIntegrationVariable { .. }
        ));
    }

    #[test]
    fn absent_integration_variable_is_still_part_of_the_context() {
        let x = symbol!("api_input_absent_x");
        let prepared = prepare_atom(&Atom::num(7), &[x]).unwrap();
        assert_eq!(prepared.context().index_of_symbol(x), Some(0));
        assert_eq!(prepared.integration_indices(), &[0]);
    }

    #[test]
    fn options_prepare_every_constant_that_period_reduction_can_introduce() {
        let x = symbol!("api_input_reduction_x");
        let options = AtomIntegrationOptions {
            mzv_reductions: MzvReductionTable {
                reductions: vec![MzvReductionRule {
                    lhs: "mzv_4".into(),
                    rhs: "2/5*mzv_2^2".into(),
                }],
                basis: vec!["Log2".into(), "mzv_2".into()],
            },
            ..AtomIntegrationOptions::default()
        };
        let prepared = prepare_atom_with_options(&Atom::one(), &[x], &options).unwrap();
        for required in [log_two_atom(), mzv_atom(&[2]), mzv_atom(&[4])] {
            assert!(
                prepared
                    .context()
                    .index_of_indeterminate(required.as_view())
                    .is_some()
            );
        }
    }
}
