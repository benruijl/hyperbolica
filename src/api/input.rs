use std::collections::HashSet;
use std::sync::Arc;

use symbolica::prelude::{Atom, AtomCore, AtomView, Symbol};

use crate::convert::{
    context_from_atom_with_indeterminates, convert_to_hlog_reg_inf,
    expression_from_atom_with_indeterminates,
};
use crate::core::{FactoredRat, PolyCtx};
use crate::integrator::{ShuffleEntry, ShuffleList};
use crate::symbols::{heads, is_library_constant};

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
    spectator_indices: Vec<usize>,
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

    /// User indeterminates that remain free throughout this integration.
    ///
    /// Generated MZV and algebraic-letter slots reserved by the options are
    /// deliberately excluded: only actual input parameters participate in
    /// divergence fibration.
    pub fn spectator_indices(&self) -> &[usize] {
        &self.spectator_indices
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

/// Lower an Atom while admitting additional native indeterminates into the
/// exact context without classifying them as option-generated constants.
///
/// Interval endpoints use this hook: a parameter that appears only in a bound
/// must still be present in the polynomial context and must remain a spectator
/// for endpoint-divergence checks.
pub(crate) fn prepare_atom_with_options_and_indeterminates(
    input: &Atom,
    integration_variables: &[Symbol],
    options: &AtomIntegrationOptions,
    additional_indeterminates: &[Atom],
) -> AtomIntegrationResult<PreparedAtomInput> {
    prepare_atom_view_with_indeterminates(
        input.as_view(),
        integration_variables,
        options,
        additional_indeterminates,
    )
}

pub(crate) fn prepare_atom_view(
    input: AtomView<'_>,
    integration_variables: &[Symbol],
    options: &AtomIntegrationOptions,
) -> AtomIntegrationResult<PreparedAtomInput> {
    prepare_atom_view_with_indeterminates(input, integration_variables, options, &[])
}

fn prepare_atom_view_with_indeterminates(
    input: AtomView<'_>,
    integration_variables: &[Symbol],
    options: &AtomIntegrationOptions,
    additional_indeterminates: &[Atom],
) -> AtomIntegrationResult<PreparedAtomInput> {
    validate_variables(integration_variables)?;
    let reserved = options.reserved_indeterminates()?;
    let mut context_indeterminates = reserved.clone();
    for indeterminate in additional_indeterminates {
        if !context_indeterminates.contains(indeterminate) {
            context_indeterminates.push(indeterminate.clone());
        }
    }
    let bare_rational = !input.contains_symbol(heads().hlog) && !input.contains_symbol(Symbol::LOG);
    let (ctx, indeterminates, shuffle_list) = if bare_rational {
        let (ctx, indeterminates) = context_from_atom_with_indeterminates(
            input,
            integration_variables,
            &context_indeterminates,
        )?;
        let coefficient = FactoredRat::from_atom(ctx.clone(), input)?;
        (
            ctx,
            indeterminates,
            vec![ShuffleEntry::from_factored(coefficient, Vec::new())],
        )
    } else {
        let lowered = expression_from_atom_with_indeterminates(
            input,
            integration_variables,
            &context_indeterminates,
        )?;
        let regulator = convert_to_hlog_reg_inf(&lowered.expr, &lowered.ctx)?;
        let shuffle_list = regulator
            .into_iter()
            .map(|term| ShuffleEntry::new(term.coef, term.key))
            .collect::<Vec<_>>();
        (lowered.ctx, lowered.indeterminates, shuffle_list)
    };
    let integration_indices = integration_variables
        .iter()
        .map(|variable| {
            ctx.index_of_symbol(*variable)
                .ok_or_else(|| crate::error::Error::UnknownVariable(variable.get_name().to_owned()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let integration_index_set = integration_indices.iter().copied().collect::<HashSet<_>>();
    let reserved = reserved.into_iter().collect::<HashSet<_>>();
    let spectator_indices = indeterminates
        .iter()
        .enumerate()
        .filter_map(|(index, atom)| {
            (!integration_index_set.contains(&index)
                && !reserved.contains(atom)
                && !is_library_constant(atom.as_view()))
            .then_some(index)
        })
        .collect();

    Ok(PreparedAtomInput {
        ctx,
        indeterminates,
        integration_variables: integration_variables.to_vec(),
        integration_indices,
        spectator_indices,
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
    fn bare_rational_atom_enters_as_a_deferred_factored_coefficient() {
        let x = symbol!("api_input_factored_x");
        let numerator = x.to_atom().pow(4) + 3 * x + 1;
        let denominator = (x + 1).pow(3) * (Atom::num(2) * x + 3).pow(2);
        let input = numerator / denominator;
        let prepared = prepare_atom(&input, &[x]).unwrap();

        let entry = &prepared.shuffle_list()[0];
        assert!(entry.coef.is_one());
        assert!(entry.shuffle.is_empty());
        let factored = entry.factored_coefficient().unwrap();
        assert!(factored.den_factors().len() >= 2);
        let mut powers = factored
            .den_factors()
            .iter()
            .map(|factor| factor.exp)
            .collect::<Vec<_>>();
        powers.sort_unstable();
        assert!(powers.ends_with(&[2, 3]));
        assert_eq!(
            factored.materialize().unwrap(),
            crate::core::Rat::from_atom(prepared.context().clone(), input.as_view()).unwrap()
        );
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
    fn builtin_log_preserves_integration_variable_dependency() {
        let x = symbol!("api_input_log_x");
        let input = x.to_atom().log();
        let prepared = prepare_atom(&input, &[x]).unwrap();
        assert_eq!(prepared.indeterminates().first(), Some(&x.to_atom()));
        assert_eq!(prepared.integration_indices(), &[0]);
        assert!(prepared.spectator_indices().is_empty());
        assert!(!prepared.shuffle_list().is_empty());
    }

    #[test]
    fn opaque_function_dependencies_fail_instead_of_becoming_constants() {
        let (x, a) = symbol!("api_input_opaque_x", "api_input_opaque_a");
        let opaque = Symbol::parse("f", "api_input_opaque").unwrap();
        let error = prepare_atom(&opaque.call(x), &[x]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("depends on integration variable")
        );

        let spectator = opaque.call(a);
        let prepared = prepare_atom(&(spectator.clone() / (x + 1)), &[x]).unwrap();
        assert!(prepared.indeterminates().contains(&spectator));
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
            mzv_reductions: MzvReductionTable::from_parts(
                vec![MzvReductionRule {
                    lhs: "mzv_4".into(),
                    rhs: "2/5*mzv_2^2".into(),
                }],
                vec!["Log2".into(), "mzv_2".into()],
            ),
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

    #[test]
    fn standard_rule_lhs_input_is_not_classified_as_a_spectator() {
        let (x, parameter) = symbol!("api_input_mzv_x", "api_input_mzv_parameter");
        let lhs = mzv_atom(&[4]);
        let input = parameter * lhs.clone() / (x + 1);
        let prepared = prepare_atom(&input, &[x]).unwrap();
        let lhs_index = prepared
            .context()
            .index_of_indeterminate(lhs.as_view())
            .unwrap();
        let parameter_index = prepared.context().index_of_symbol(parameter).unwrap();

        assert!(!prepared.spectator_indices().contains(&lhs_index));
        assert!(prepared.spectator_indices().contains(&parameter_index));
        assert!(prepared.context().len() < 20);
    }

    #[test]
    fn arbitrary_registered_constants_are_not_spectator_variables() {
        let (x, parameter) = symbol!(
            "api_input_registered_constant_x",
            "api_input_registered_constant_parameter"
        );
        let unknown_mzv = mzv_atom(&[999]);
        let algebraic = crate::symbols::algebraic_atoms(77).minus;
        let input = parameter * unknown_mzv.clone() * algebraic.clone() / (x + 1);
        let prepared = prepare_atom(&input, &[x]).unwrap();

        let parameter_index = prepared.context().index_of_symbol(parameter).unwrap();
        let mzv_index = prepared
            .context()
            .index_of_indeterminate(unknown_mzv.as_view())
            .unwrap();
        let algebraic_index = prepared
            .context()
            .index_of_indeterminate(algebraic.as_view())
            .unwrap();
        assert!(prepared.spectator_indices().contains(&parameter_index));
        assert!(!prepared.spectator_indices().contains(&mzv_index));
        assert!(!prepared.spectator_indices().contains(&algebraic_index));
    }
}
