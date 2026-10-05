use std::collections::HashSet;

use symbolica::prelude::{Atom, AtomCore, Symbol};

use crate::{
    core::Rat,
    integrator::{
        ShuffleList, hyperflint_with_options_and_spectators, rescale_interval_with_bound_parser,
    },
};

use super::{
    AtomIntegrationOptions, AtomIntegrationOutput, AtomIntegrationResult, IntegrationEndpoint,
    IntegrationInterval, PreparedAtomInput, input::prepare_atom_with_options_and_indeterminates,
    prepare_atom_with_options,
};

const NATIVE_FROM_BOUND: &str = "\0hyperbolica-native-from-bound";
const NATIVE_TO_BOUND: &str = "\0hyperbolica-native-to-bound";

fn validate_interval_count(
    variable_count: usize,
    intervals: &[IntegrationInterval],
) -> AtomIntegrationResult<()> {
    if intervals.len() != variable_count {
        return Err(crate::error::Error::InvalidInput(format!(
            "integration interval count {} does not match integration variable count {variable_count}",
            intervals.len()
        ))
        .into());
    }
    Ok(())
}

fn interval_indeterminates(
    intervals: &[IntegrationInterval],
    integration_variables: &[Symbol],
) -> AtomIntegrationResult<Vec<Atom>> {
    let mut discovered = HashSet::new();
    for interval in intervals {
        for endpoint in [&interval.from, &interval.to] {
            if let Some(value) = endpoint.as_finite() {
                discovered.extend(
                    value
                        .as_view()
                        .get_all_indeterminates(false)
                        .into_iter()
                        .map(|atom| atom.to_owned()),
                );
            }
        }
    }
    for candidate in &discovered {
        for variable in integration_variables {
            let variable_atom = variable.to_atom();
            if candidate != &variable_atom && candidate.contains_symbol(*variable) {
                return Err(crate::error::Error::InvalidInput(format!(
                    "unsupported interval-bound indeterminate `{candidate}` depends on integration variable `{}`",
                    variable.get_name()
                ))
                .into());
            }
        }
    }
    let mut ordered = discovered.into_iter().collect::<Vec<_>>();
    ordered.sort();
    Ok(ordered)
}

fn validate_prepared_context(
    prepared: &PreparedAtomInput,
    options: &AtomIntegrationOptions,
    interval_indeterminates: &[Atom],
) -> AtomIntegrationResult<()> {
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
    for required in interval_indeterminates {
        if prepared
            .context()
            .index_of_indeterminate(required.as_view())
            .is_none()
        {
            return Err(crate::error::Error::InvalidInput(format!(
                "prepared Atom context does not contain interval-bound indeterminate `{required}`; prepare with `prepare_atom_over`"
            ))
            .into());
        }
    }
    Ok(())
}

fn native_endpoint(
    endpoint: &IntegrationEndpoint,
    finite_marker: &'static str,
    context: &std::sync::Arc<crate::core::PolyCtx>,
) -> AtomIntegrationResult<(&'static str, Option<Rat>)> {
    match endpoint {
        IntegrationEndpoint::Finite(value) => Ok((
            finite_marker,
            Some(Rat::from_atom(context.clone(), value.as_view())?),
        )),
        IntegrationEndpoint::PositiveInfinity => Ok(("Infinity", None)),
        IntegrationEndpoint::NegativeInfinity => Ok(("-Infinity", None)),
    }
}

fn rescale_prepared_intervals(
    prepared: &PreparedAtomInput,
    intervals: &[IntegrationInterval],
) -> AtomIntegrationResult<ShuffleList> {
    let mut input = prepared.shuffle_list().clone();
    for (&variable, interval) in prepared.integration_indices().iter().zip(intervals) {
        let (from, from_value) =
            native_endpoint(&interval.from, NATIVE_FROM_BOUND, prepared.context())?;
        let (to, to_value) = native_endpoint(&interval.to, NATIVE_TO_BOUND, prepared.context())?;
        input = rescale_interval_with_bound_parser(
            prepared.context(),
            &input,
            variable,
            from,
            to,
            |marker| match marker {
                NATIVE_FROM_BOUND => from_value.clone().ok_or_else(|| {
                    crate::error::Error::InvalidInput(
                        "missing native finite lower integration bound".into(),
                    )
                }),
                NATIVE_TO_BOUND => to_value.clone().ok_or_else(|| {
                    crate::error::Error::InvalidInput(
                        "missing native finite upper integration bound".into(),
                    )
                }),
                _ => Err(crate::error::Error::InvalidInput(format!(
                    "unknown native integration-bound marker `{marker}`"
                ))),
            },
        )?;
        if input.is_empty() {
            break;
        }
    }
    Ok(input)
}

fn integrate_prepared_shuffle_list(
    prepared: &PreparedAtomInput,
    input: &ShuffleList,
    options: &AtomIntegrationOptions,
) -> AtomIntegrationResult<AtomIntegrationOutput> {
    let _algebraic_session = options
        .introduce_algebraic_letters
        .then(crate::algebra::begin_algebraic_letter_session)
        .transpose()?;
    let regulator = hyperflint_with_options_and_spectators(
        prepared.context(),
        input,
        prepared.integration_indices(),
        &options.mzv_reductions,
        &options.core_options(),
        prepared.spectator_indices(),
    )?;
    // The engine may leave evaluable infinity periods in its regulator keys.
    // Reuse the period layer with the caller's MZV table before materializing
    // the public expression; nonconstant periods retain their exact words.
    let regulator = crate::reduce::fibration_basis_sym(
        prepared.context(),
        &regulator,
        &[],
        &options.mzv_reductions,
    )?
    .terms
    .into_iter()
    .map(|(key, coef)| crate::integrator::RegTermSym { key, coef })
    .collect();
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

/// Run the full integration pipeline on an already lowered Atom input.
pub fn integrate_prepared_atom(
    prepared: &PreparedAtomInput,
    options: &AtomIntegrationOptions,
) -> AtomIntegrationResult<AtomIntegrationOutput> {
    validate_prepared_context(prepared, options, &[])?;
    integrate_prepared_shuffle_list(prepared, prepared.shuffle_list(), options)
}

/// Integrate an already lowered Atom input over directed intervals.
///
/// `intervals` is aligned one-for-one with the prepared integration-variable
/// schedule. Every rescaling is performed before the first integration step,
/// matching the upstream HyperFLINT driver.
pub fn integrate_prepared_atom_over(
    prepared: &PreparedAtomInput,
    intervals: &[IntegrationInterval],
    options: &AtomIntegrationOptions,
) -> AtomIntegrationResult<AtomIntegrationOutput> {
    validate_interval_count(prepared.integration_indices().len(), intervals)?;
    let interval_indeterminates =
        interval_indeterminates(intervals, prepared.integration_variables())?;
    validate_prepared_context(prepared, options, &interval_indeterminates)?;
    let input = rescale_prepared_intervals(prepared, intervals)?;
    integrate_prepared_shuffle_list(prepared, &input, options)
}

/// Lower an Atom with every finite interval-bound parameter in its exact
/// context, ready for [`integrate_prepared_atom_over`].
pub fn prepare_atom_over(
    input: &Atom,
    integration_variables: &[Symbol],
    intervals: &[IntegrationInterval],
    options: &AtomIntegrationOptions,
) -> AtomIntegrationResult<PreparedAtomInput> {
    validate_interval_count(integration_variables.len(), intervals)?;
    let interval_indeterminates = interval_indeterminates(intervals, integration_variables)?;
    prepare_atom_with_options_and_indeterminates(
        input,
        integration_variables,
        options,
        &interval_indeterminates,
    )
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

/// Integrate a concrete Symbolica Atom over arbitrary directed intervals.
///
/// The interval slice is aligned with `integration_variables`. Finite bounds
/// are native Symbolica expressions; no mathematical input is formatted and
/// reparsed at this API boundary.
pub fn integrate_atom_over(
    input: &Atom,
    integration_variables: &[Symbol],
    intervals: &[IntegrationInterval],
    options: &AtomIntegrationOptions,
) -> AtomIntegrationResult<AtomIntegrationOutput> {
    let prepared = prepare_atom_over(input, integration_variables, intervals, options)?;
    integrate_prepared_atom_over(&prepared, intervals, options)
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
    fn massless_two_loop_kite_has_the_correct_weight_three_period() {
        let (x1, x2, x3, x4) = symbol!(
            "kite_regression::x1",
            "kite_regression::x2",
            "kite_regression::x3",
            "kite_regression::x4"
        );
        // D=4, p^2=-1, five unit propagator powers, projective gauge x5=1.
        let u = (x1 + x2) * (x3 + x4) + x1 + x2 + x3 + x4;
        let f = (x1 * x2) * (x3 + x4 + 1) + (x3 * x4) * (x1 + x2 + 1) + x1 * x4 + x2 * x3;
        let input = Atom::one() / (u * f);
        let options = AtomIntegrationOptions {
            parallel: false,
            check_divergences: true,
            ..AtomIntegrationOptions::default()
        };
        for order in [[x1, x2, x3, x4], [x4, x3, x2, x1]] {
            let result = integrate_atom(&input, &order, &options)
                .unwrap()
                .to_atom()
                .unwrap();
            assert_eq!(result, Atom::num(6) * crate::symbols::mzv_atom(&[3]));
        }
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

    #[test]
    fn finite_directed_intervals_include_the_exact_jacobian() {
        let x = symbol!("api_integrate_finite_interval_x");
        let options = AtomIntegrationOptions {
            parallel: false,
            check_divergences: true,
            ..AtomIntegrationOptions::default()
        };
        let forward = integrate_atom_over(
            &Atom::one(),
            &[x],
            &[IntegrationInterval::finite(2, 5)],
            &options,
        )
        .unwrap()
        .to_atom()
        .unwrap();
        let backward = integrate_atom_over(
            &Atom::one(),
            &[x],
            &[IntegrationInterval::finite(5, 2)],
            &options,
        )
        .unwrap()
        .to_atom()
        .unwrap();
        assert_eq!(forward, Atom::num(3));
        assert_eq!(backward, Atom::num(-3));
    }

    #[test]
    fn multivariable_bounds_are_rescaled_in_upstream_schedule_order() {
        let (x, y) = symbol!(
            "api_integrate_nested_interval_x",
            "api_integrate_nested_interval_y"
        );
        let options = AtomIntegrationOptions {
            parallel: false,
            check_divergences: true,
            ..AtomIntegrationOptions::default()
        };
        let result = integrate_atom_over(
            &Atom::one(),
            &[x, y],
            &[
                IntegrationInterval::finite(Atom::zero(), y),
                IntegrationInterval::finite(0, 1),
            ],
            &options,
        )
        .unwrap()
        .to_atom()
        .unwrap();
        assert_eq!(result, Atom::num(1) / Atom::num(2));
    }

    #[test]
    fn bound_only_parameters_are_context_spectators() {
        let (x, a) = symbol!(
            "api_integrate_parameter_bound_x",
            "api_integrate_parameter_bound_a"
        );
        let interval = IntegrationInterval::new(
            IntegrationEndpoint::finite(a),
            IntegrationEndpoint::PositiveInfinity,
        );
        let options = AtomIntegrationOptions {
            parallel: false,
            check_divergences: true,
            ..AtomIntegrationOptions::default()
        };
        let input = Atom::one() / (x + 1).pow(2);
        let prepared =
            prepare_atom_over(&input, &[x], std::slice::from_ref(&interval), &options).unwrap();
        let parameter_index = prepared.context().index_of_symbol(a).unwrap();
        assert!(prepared.spectator_indices().contains(&parameter_index));

        let result = integrate_prepared_atom_over(&prepared, &[interval], &options)
            .unwrap()
            .to_atom()
            .unwrap();
        assert_eq!(result, Atom::one() / (a + 1));
    }

    #[test]
    fn endpoint_matrix_matches_the_upstream_rescaler() {
        let x = symbol!("api_integrate_endpoint_matrix_x");
        let options = AtomIntegrationOptions::default();
        let finite = |value| IntegrationEndpoint::finite(Atom::num(value));
        let cases = [
            (
                IntegrationInterval::new(IntegrationEndpoint::NegativeInfinity, finite(0)),
                -1,
                1,
            ),
            (
                IntegrationInterval::new(finite(0), IntegrationEndpoint::NegativeInfinity),
                -1,
                1,
            ),
            (
                IntegrationInterval::new(IntegrationEndpoint::PositiveInfinity, finite(0)),
                -1,
                1,
            ),
            (
                IntegrationInterval::new(finite(0), IntegrationEndpoint::PositiveInfinity),
                1,
                1,
            ),
            (IntegrationInterval::real_line(), 1, 2),
        ];
        for (interval, expected_coefficient, expected_entries) in cases {
            let prepared = prepare_atom_over(
                &Atom::one(),
                &[x],
                std::slice::from_ref(&interval),
                &options,
            )
            .unwrap();
            let rescaled = rescale_prepared_intervals(&prepared, &[interval]).unwrap();
            assert_eq!(rescaled.len(), expected_entries);
            assert!(rescaled.iter().all(|entry| {
                entry.materialized_coefficient().unwrap()
                    == Rat::from_int(prepared.context().clone(), expected_coefficient)
            }));
        }

        let equal = IntegrationInterval::finite(7, 7);
        let prepared =
            prepare_atom_over(&Atom::one(), &[x], std::slice::from_ref(&equal), &options).unwrap();
        assert!(
            rescale_prepared_intervals(&prepared, &[equal])
                .unwrap()
                .is_empty()
        );

        let unsupported = IntegrationInterval::new(
            IntegrationEndpoint::PositiveInfinity,
            IntegrationEndpoint::NegativeInfinity,
        );
        let prepared = prepare_atom_over(
            &Atom::one(),
            &[x],
            std::slice::from_ref(&unsupported),
            &options,
        )
        .unwrap();
        assert!(
            rescale_prepared_intervals(&prepared, &[unsupported])
                .unwrap_err()
                .to_string()
                .contains("unsupported interval bound combination")
        );
    }

    #[test]
    fn interval_schedule_length_is_validated_before_lowering() {
        let x = symbol!("api_integrate_interval_count_x");
        let error =
            integrate_atom_over(&Atom::one(), &[x], &[], &AtomIntegrationOptions::default())
                .unwrap_err();
        assert!(error.to_string().contains("interval count 0"));
    }

    #[test]
    fn opaque_bound_dependencies_do_not_become_constant_coefficients() {
        let (x, y) = symbol!(
            "api_integrate_opaque_bound_x",
            "api_integrate_opaque_bound_y"
        );
        let opaque = Symbol::parse("f", "api_integrate_opaque_bound").unwrap();
        let interval = IntegrationInterval::finite(Atom::zero(), opaque.call(y));
        let error = prepare_atom_over(
            &Atom::one(),
            &[x, y],
            &[interval, IntegrationInterval::finite(0, 1)],
            &AtomIntegrationOptions::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("interval-bound indeterminate"));
    }
}
