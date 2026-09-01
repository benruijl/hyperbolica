use symbolica::prelude::{Atom, Symbol};

use crate::integrator::hyperflint_with_options_and_spectators;

use super::{
    AtomIntegrationOptions, AtomIntegrationOutput, AtomIntegrationResult, PreparedAtomInput,
    prepare_atom_with_options,
};

/// Run the full integration pipeline on an already lowered Atom input.
pub fn integrate_prepared_atom(
    prepared: &PreparedAtomInput,
    options: &AtomIntegrationOptions,
) -> AtomIntegrationResult<AtomIntegrationOutput> {
    for required in options.reserved_indeterminates()? {
        if prepared
            .context()
            .index_of_indeterminate(required.as_view())
            .is_none()
        {
            return Err(crate::error::Error::InvalidInput(format!(
                "prepared Atom context does not reserve option-introduced indeterminate `{required}`; prepare again with the same options"
            ))
            .into());
        }
    }
    let _algebraic_session = options
        .introduce_algebraic_letters
        .then(crate::algebra::begin_algebraic_letter_session)
        .transpose()?;
    let regulator = hyperflint_with_options_and_spectators(
        prepared.context(),
        prepared.shuffle_list(),
        prepared.integration_indices(),
        &options.mzv_reductions,
        &options.core_options(),
        prepared.spectator_indices(),
    )?;
    let algebraic_letters = if options.introduce_algebraic_letters {
        crate::algebra::algebraic_letters_show()?
    } else {
        Vec::new()
    };
    Ok(AtomIntegrationOutput::from_regulator(
        prepared.context().clone(),
        prepared.integration_variables().to_vec(),
        prepared.indeterminates().to_vec(),
        algebraic_letters,
        regulator,
    ))
}

/// Integrate a concrete Symbolica Atom over `[0, infinity)`.
///
/// This is the primary public API.  Its input remains a Symbolica expression
/// throughout lowering; string parsing is confined to separate compatibility
/// adapters.
pub fn integrate_atom(
    input: &Atom,
    integration_variables: &[Symbol],
    options: &AtomIntegrationOptions,
) -> AtomIntegrationResult<AtomIntegrationOutput> {
    let prepared = prepare_atom_with_options(input, integration_variables, options)?;
    integrate_prepared_atom(&prepared, options)
}

#[cfg(test)]
mod tests {
    use symbolica::prelude::{Atom, AtomCore, symbol};

    use super::*;

    #[test]
    fn empty_schedule_round_trips_a_rational_atom() {
        let x = symbol!("api_integrate_roundtrip_x");
        let input = (x + 1) / 3;
        let output = integrate_atom(&input, &[], &AtomIntegrationOptions::default()).unwrap();
        assert_eq!(output.to_atom().unwrap(), input);
    }

    #[test]
    fn rational_integral_runs_through_the_complete_pipeline() {
        let x = symbol!("api_integrate_rational_x");
        let input = Atom::one() / (x + 1).pow(2);
        let options = AtomIntegrationOptions {
            parallel: false,
            check_divergences: true,
            ..AtomIntegrationOptions::default()
        };
        let output = integrate_atom(&input, &[x], &options).unwrap();
        assert_eq!(output.to_atom().unwrap(), Atom::one());
        assert_eq!(output.integration_variables(), &[x]);
    }

    #[test]
    fn builtin_log_integral_is_not_treated_as_a_constant_coefficient() {
        let x = symbol!("api_integrate_log_x");
        let input = x.to_atom().log() / (x + 1).pow(2);
        let options = AtomIntegrationOptions {
            parallel: false,
            check_divergences: true,
            ..AtomIntegrationOptions::default()
        };
        let output = integrate_atom(&input, &[x], &options).unwrap();
        assert!(output.is_zero());
        assert_eq!(output.to_atom().unwrap(), Atom::zero());
    }
}
