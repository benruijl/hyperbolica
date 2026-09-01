use std::sync::Arc;

use symbolica::prelude::{Atom, AtomCore, Symbol};

use crate::algebra::AlgebraicLetterEntry;
use crate::core::{PolyCtx, SymCoef, SymMonomial};
use crate::integrator::RegulatorSym;
use crate::symbols::{Word, heads};

use super::AtomIntegrationResult;

/// One collected term in an Atom-native integration result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtomIntegrationTerm {
    pub coefficient: SymCoef,
    /// A commutative product of period factors.  Each word is represented as
    /// `Hlog(1, a1, ..., an)` by [`Self::to_atom`].
    pub periods: Vec<Word>,
}

impl AtomIntegrationTerm {
    pub fn to_atom(&self) -> AtomIntegrationResult<Atom> {
        let coefficient = symcoef_to_atom(&self.coefficient)?;
        Ok(self.periods.iter().fold(coefficient, |product, word| {
            product * period_word_to_atom(word)
        }))
    }
}

/// Collected output of the full integration pipeline.
#[derive(Clone, Debug)]
pub struct AtomIntegrationOutput {
    ctx: Arc<PolyCtx>,
    integration_variables: Vec<Symbol>,
    indeterminates: Vec<Atom>,
    algebraic_letters: Vec<AlgebraicLetterEntry>,
    terms: Vec<AtomIntegrationTerm>,
}

impl AtomIntegrationOutput {
    pub(crate) fn from_regulator(
        ctx: Arc<PolyCtx>,
        integration_variables: Vec<Symbol>,
        indeterminates: Vec<Atom>,
        algebraic_letters: Vec<AlgebraicLetterEntry>,
        regulator: RegulatorSym,
    ) -> Self {
        Self {
            ctx,
            integration_variables,
            indeterminates,
            algebraic_letters,
            terms: regulator
                .into_iter()
                .map(|term| AtomIntegrationTerm {
                    coefficient: term.coef,
                    periods: term.key,
                })
                .collect(),
        }
    }

    pub fn context(&self) -> &Arc<PolyCtx> {
        &self.ctx
    }

    pub fn integration_variables(&self) -> &[Symbol] {
        &self.integration_variables
    }

    pub fn indeterminates(&self) -> &[Atom] {
        &self.indeterminates
    }

    /// Formal quadratic roots allocated during this integration, in stable
    /// one-based index order.
    pub fn algebraic_letters(&self) -> &[AlgebraicLetterEntry] {
        &self.algebraic_letters
    }

    pub fn terms(&self) -> &[AtomIntegrationTerm] {
        &self.terms
    }

    pub fn is_zero(&self) -> bool {
        self.terms.is_empty()
    }

    /// Materialize the collected result as one normalized Symbolica Atom.
    pub fn to_atom(&self) -> AtomIntegrationResult<Atom> {
        self.terms
            .iter()
            .map(AtomIntegrationTerm::to_atom)
            .collect::<AtomIntegrationResult<Vec<_>>>()
            .map(|terms| terms.into_iter().sum())
    }
}

/// Convert a residual period word to the canonical public Atom form.
pub fn period_word_to_atom(word: &Word) -> Atom {
    if word.is_empty() {
        return Atom::one();
    }
    heads().hlog.call_args(
        std::iter::once(Atom::one()).chain(word.letters.iter().map(|letter| letter.to_atom())),
    )
}

fn power(base: Atom, exponent: i32) -> Atom {
    if exponent == 1 {
        base
    } else {
        base.pow(exponent)
    }
}

fn monomial_to_atom(ctx: &PolyCtx, monomial: &SymMonomial) -> AtomIntegrationResult<Atom> {
    let mut factors = Vec::with_capacity(
        3 + monomial.log_powers.len() + monomial.delta_powers.len() + monomial.period_powers.len(),
    );
    factors.push(monomial.prefactor.to_atom());
    if monomial.pi_power != 0 {
        factors.push(power(Symbol::PI.to_atom(), monomial.pi_power));
    }
    if monomial.i_power != 0 {
        factors.push(power(Atom::i(), monomial.i_power));
    }
    for (&argument, &exponent) in &monomial.log_powers {
        factors.push(power(Symbol::LOG.call(argument), exponent));
    }
    for (variable, &exponent) in &monomial.delta_powers {
        let variable = ctx.variable_atom(*variable)?;
        factors.push(power(heads().delta.call(variable), exponent));
    }
    for (&period, &exponent) in &monomial.period_powers {
        factors.push(power(heads().period.call(period), exponent));
    }
    Ok(factors.into_iter().product())
}

/// Convert an exact symbolic coefficient into a normalized Symbolica Atom.
///
/// Built-in Symbolica heads are used for `Pi`, `I`, and `log`; every
/// Hyperbolica-owned head comes from the single registry in `symbols`.
pub fn symcoef_to_atom(coefficient: &SymCoef) -> AtomIntegrationResult<Atom> {
    coefficient
        .terms()
        .iter()
        .map(|monomial| monomial_to_atom(coefficient.ctx(), monomial))
        .collect::<AtomIntegrationResult<Vec<_>>>()
        .map(|terms| terms.into_iter().sum())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use symbolica::prelude::{AtomCore, symbol};

    use crate::core::{PolyCtx, Rat};

    use super::*;

    #[test]
    fn period_words_use_the_registered_hlog_head_and_unit_endpoint() {
        let ctx = PolyCtx::from_symbols([symbol!("api_output_word_x")]).unwrap();
        let word = Word::new(vec![Rat::from_int(ctx, -1)]);
        let atom = period_word_to_atom(&word);
        let function = atom.as_fun_view().unwrap();
        assert_eq!(function.get_symbol(), heads().hlog);
        assert_eq!(function.get_nargs(), 2);
        assert!(function.get(0).is_one());
    }

    #[test]
    fn symbolic_coefficient_uses_builtin_and_registered_heads() {
        let x = symbol!("api_output_coef_x");
        let ctx = PolyCtx::from_symbols([x]).unwrap();
        let mut monomial = SymMonomial::new(Rat::from_int(ctx.clone(), 3));
        monomial.pi_power = 2;
        monomial.i_power = 1;
        monomial.log_powers = BTreeMap::from([(2, 1)]);
        monomial.delta_powers.insert(0, 1);
        monomial.period_powers = BTreeMap::from([(7, 2)]);
        let coefficient = SymCoef::from_monomials(ctx, vec![monomial]);
        let atom = symcoef_to_atom(&coefficient).unwrap();
        assert!(atom.contains_symbol(Symbol::PI));
        assert!(atom.contains_symbol(Symbol::LOG));
        assert!(atom.contains_symbol(heads().delta));
        assert!(atom.contains_symbol(heads().period));
    }

    #[test]
    fn output_sums_terms_and_skips_identity_periods() {
        let ctx = PolyCtx::from_symbols([symbol!("api_output_sum_x")]).unwrap();
        let output = AtomIntegrationOutput::from_regulator(
            ctx.clone(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![
                crate::integrator::RegTermSym {
                    coef: SymCoef::from_rat(&Rat::from_int(ctx.clone(), 2)),
                    key: Vec::new(),
                },
                crate::integrator::RegTermSym {
                    coef: SymCoef::from_rat(&Rat::from_int(ctx.clone(), 3)),
                    key: vec![Word::new(vec![Rat::from_int(ctx, -1)])],
                },
            ],
        );
        let atom = output.to_atom().unwrap();
        assert!(atom.contains_symbol(heads().hlog));
        assert_eq!(output.terms().len(), 2);
    }

    #[test]
    fn unknown_delta_variables_fail_at_construction() {
        let ctx = PolyCtx::from_symbols([symbol!("api_output_delta_x")]).unwrap();
        assert!(matches!(
            SymCoef::delta_factor(ctx.clone(), ctx.len()),
            Err(crate::error::Error::UnknownVariable(_))
        ));
    }

    #[test]
    fn delta_atom_output_preserves_namespaced_variable_identity() {
        let left = Symbol::parse("x", "api_delta_left").unwrap();
        let right = Symbol::parse("x", "api_delta_right").unwrap();
        let ctx = PolyCtx::from_indeterminates([left.to_atom(), right.to_atom()]).unwrap();
        assert_eq!(ctx.vars(), &["x", "x"]);

        let coefficient = SymCoef::delta_factor(ctx, 1).unwrap();
        let atom = symcoef_to_atom(&coefficient).unwrap();
        let delta = atom.as_fun_view().expect("one delta factor is a function");
        assert_eq!(delta.get_symbol(), heads().delta);
        assert_eq!(delta.get_nargs(), 1);
        assert_eq!(delta.get(0), right.to_atom().as_view());
        assert_ne!(delta.get(0), left.to_atom().as_view());
    }
}
