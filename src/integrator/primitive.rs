use std::collections::{HashMap, VecDeque, hash_map::RandomState};
use std::hash::BuildHasher;
use std::sync::Arc;

use super::accumulator::BalancedSum;
use crate::algebra::partial_fractions::{
    PartialFractionOptions, partial_fractions_factored_with_options, partial_fractions_with_options,
};
use crate::core::{FactoredRat, Poly, PolyCtx, Rat};
use crate::error::{Error, Result};
use crate::symbols::{Word, Wordlist, WordlistTerm};

/// Options for one primitive construction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IntegrateIiOptions<'a> {
    pub introduce_algebraic_letters: bool,
    pub forbidden_variables: &'a [usize],
}

fn antiderivative_polynomial_part(polynomial: &Rat, variable: usize) -> Result<Rat> {
    polynomial.integrate_polynomial_part(variable)
}

struct AccumulatorCell {
    word: Word,
    coefficient: BalancedSum<Rat>,
}

#[derive(Clone)]
enum WorkCoefficient {
    Rational(Rat),
    Factored(FactoredRat),
}

impl WorkCoefficient {
    fn ctx(&self) -> &Arc<PolyCtx> {
        match self {
            Self::Rational(value) => value.ctx(),
            Self::Factored(value) => value.ctx(),
        }
    }
}

#[derive(Clone)]
struct WorkTerm {
    coefficient: WorkCoefficient,
    word: Word,
}

/// A collision-safe index into [`AccumulatorCell`] rows.
///
/// `Word` contains `Rat`, whose compatibility views are initialized lazily.
/// Keeping `Word` itself in a `HashMap` would therefore make the map key carry
/// interior mutability. Instead, this index maps a structural hash to every
/// candidate row and resolves collisions with the full `Word` equality check.
/// The rows remain the source of truth; the digest is never treated as
/// identity.
struct WordIndex<S = RandomState> {
    buckets: HashMap<u64, Vec<usize>>,
    hash_builder: S,
}

impl Default for WordIndex<RandomState> {
    fn default() -> Self {
        Self::with_hasher(RandomState::new())
    }
}

impl<S: BuildHasher> WordIndex<S> {
    fn with_hasher(hash_builder: S) -> Self {
        Self {
            buckets: HashMap::new(),
            hash_builder,
        }
    }

    fn find(&self, rows: &[AccumulatorCell], word: &Word) -> (u64, Option<usize>) {
        let digest = self.hash_builder.hash_one(word);
        let row = self.buckets.get(&digest).and_then(|candidates| {
            candidates
                .iter()
                .copied()
                .find(|&index| rows[index].word == *word)
        });
        (digest, row)
    }

    fn insert(&mut self, digest: u64, row: usize) {
        self.buckets.entry(digest).or_default().push(row);
    }
}

fn bump<S: BuildHasher>(
    rows: &mut Vec<AccumulatorCell>,
    indices: &mut WordIndex<S>,
    word: Word,
    coefficient: Rat,
) -> Result<()> {
    if coefficient.is_zero() {
        return Ok(());
    }
    let (digest, existing) = indices.find(rows, &word);
    if let Some(index) = existing {
        rows[index].coefficient.push(coefficient)?;
    } else {
        indices.insert(digest, rows.len());
        rows.push(AccumulatorCell {
            word,
            coefficient: BalancedSum::new(coefficient),
        });
    }
    Ok(())
}

fn push_integration_by_parts(
    queue: &mut VecDeque<WorkTerm>,
    primitive: &Rat,
    word: &Word,
    variable_rat: &Rat,
) -> Result<()> {
    let Some(first_letter) = word.letters.first() else {
        return Ok(());
    };
    let chain_denominator = variable_rat.try_sub(first_letter)?;
    if chain_denominator.is_zero() {
        return Ok(());
    }
    let coefficient = primitive.negated().try_div(&chain_denominator)?;
    if !coefficient.is_zero() {
        queue.push_back(WorkTerm {
            coefficient: WorkCoefficient::Rational(coefficient),
            word: Word::from(word.letters[1..].to_vec()),
        });
    }
    Ok(())
}

/// Integrate a wordlist in the HyperFLINT hyperlogarithm basis.
///
/// Partial fractions turn simple poles into prepended letters. Polynomial
/// parts and higher-order poles are integrated rationally and their
/// integration-by-parts corrections are appended to the work queue.
pub fn integrate_ii(ctx: &Arc<PolyCtx>, wordlist: &Wordlist, variable: usize) -> Result<Wordlist> {
    integrate_ii_with_options(ctx, wordlist, variable, &IntegrateIiOptions::default())
}

/// Integrate a wordlist with optional formal roots for quadratic poles.
pub fn integrate_ii_with_options(
    ctx: &Arc<PolyCtx>,
    wordlist: &Wordlist,
    variable: usize,
    options: &IntegrateIiOptions<'_>,
) -> Result<Wordlist> {
    let queue = wordlist
        .terms
        .iter()
        .map(|term| WorkTerm {
            coefficient: WorkCoefficient::Rational(term.coef.clone()),
            word: term.word.clone(),
        })
        .collect();
    integrate_ii_work_queue(ctx, queue, variable, options)
}

/// Multiply the initial wordlist by one deferred rational prefactor.
///
/// Canonical denominator construction is deferred to the first partial-
/// fraction decomposition. That step separates coprime target-dependent
/// blocks first, then eagerly materializes and decomposes each component; an
/// overlapping-block input uses the exact full-materialization fallback.
pub(crate) fn integrate_ii_with_factored_prefactor(
    ctx: &Arc<PolyCtx>,
    wordlist: &Wordlist,
    prefactor: &FactoredRat,
    variable: usize,
    options: &IntegrateIiOptions<'_>,
) -> Result<Wordlist> {
    let queue = wordlist
        .terms
        .iter()
        .map(|term| {
            if !term.coef.ctx().is_compatible_with(prefactor.ctx()) {
                return Err(Error::ContextMismatch);
            }
            let coefficient = if term.coef.is_zero() {
                FactoredRat::from_poly(Poly::zero(prefactor.ctx().clone()))
            } else if term.coef.is_one() {
                prefactor.clone()
            } else {
                FactoredRat::from_rat(&term.coef).try_mul(prefactor)?
            };
            Ok(WorkTerm {
                coefficient: WorkCoefficient::Factored(coefficient),
                word: term.word.clone(),
            })
        })
        .collect::<Result<VecDeque<_>>>()?;
    integrate_ii_work_queue(ctx, queue, variable, options)
}

fn integrate_ii_work_queue(
    ctx: &Arc<PolyCtx>,
    mut queue: VecDeque<WorkTerm>,
    variable: usize,
    options: &IntegrateIiOptions<'_>,
) -> Result<Wordlist> {
    let variable_rat = Rat::from_poly(Poly::generator(ctx.clone(), variable)?);
    let mut rows = Vec::<AccumulatorCell>::new();
    let mut indices = WordIndex::default();

    // Integration-by-parts corrections append to the same FIFO. Consuming
    // entries releases their coefficients instead of retaining every already
    // processed term and cloning it a second time.
    while let Some(term) = queue.pop_front() {
        if !term.coefficient.ctx().is_compatible_with(ctx)
            || term
                .word
                .letters
                .iter()
                .any(|letter| !letter.ctx().is_compatible_with(ctx))
        {
            return Err(Error::ContextMismatch);
        }

        let partial_fraction_options = PartialFractionOptions {
            introduce_algebraic_letters: options.introduce_algebraic_letters,
            forbidden_variables: options.forbidden_variables,
        };
        let fractions = match &term.coefficient {
            WorkCoefficient::Rational(coefficient) => {
                partial_fractions_with_options(coefficient, variable, &partial_fraction_options)
            }
            WorkCoefficient::Factored(coefficient) => partial_fractions_factored_with_options(
                coefficient,
                variable,
                &partial_fraction_options,
            ),
        }
        .map_err(|error| Error::InvalidInput(format!("IntegrateII: partial_fractions: {error}")))?;

        if !fractions.polynomial_part.is_zero() {
            let primitive = antiderivative_polynomial_part(&fractions.polynomial_part, variable)?;
            bump(
                &mut rows,
                &mut indices,
                term.word.clone(),
                primitive.clone(),
            )?;
            push_integration_by_parts(&mut queue, &primitive, &term.word, &variable_rat)?;
        }

        for pole in fractions.poles {
            for (coefficient_index, coefficient) in pole.coefs.into_iter().enumerate() {
                if coefficient.is_zero() {
                    continue;
                }
                let order = coefficient_index + 1;
                if order == 1 {
                    let mut letters = Vec::with_capacity(term.word.len() + 1);
                    letters.push(pole.pole.clone());
                    letters.extend(term.word.letters.iter().cloned());
                    bump(&mut rows, &mut indices, Word::from(letters), coefficient)?;
                    continue;
                }

                let exponent = i64::try_from(order - 1)
                    .map_err(|_| Error::InvalidInput("pole order does not fit in i64".into()))?;
                let one_minus_order = 1_i64
                    .checked_sub(i64::try_from(order).map_err(|_| {
                        Error::InvalidInput("pole order does not fit in i64".into())
                    })?)
                    .ok_or_else(|| Error::InvalidInput("pole order overflow".into()))?;
                let denominator = variable_rat
                    .try_sub(&pole.pole)?
                    .pow(exponent)?
                    .try_mul(&Rat::from_int(ctx.clone(), one_minus_order))?;
                let primitive = coefficient.try_div(&denominator)?;
                bump(
                    &mut rows,
                    &mut indices,
                    term.word.clone(),
                    primitive.clone(),
                )?;
                push_integration_by_parts(&mut queue, &primitive, &term.word, &variable_rat)?;
            }
        }
    }

    let mut output = Wordlist::default();
    for row in rows {
        let coefficient = row.coefficient.finish()?;
        if !coefficient.is_zero() {
            output.terms.push(WordlistTerm::new(coefficient, row.word));
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::hash::{BuildHasherDefault, Hasher};

    use super::*;
    use symbolica::prelude::{AtomCore, Symbol};

    use crate::algebra::{
        DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE, begin_algebraic_letter_session,
        build_algebraic_letter_atom_list,
    };
    use crate::integrator::differentiate_wordlist;
    use crate::symbols::SYMBOL_NAMESPACE;

    fn algebraic_context() -> Arc<PolyCtx> {
        let variables = ["primitive_alg_x", "primitive_alg_a"]
            .into_iter()
            .map(|name| Symbol::parse(name, SYMBOL_NAMESPACE).unwrap().to_atom());
        PolyCtx::from_indeterminates(build_algebraic_letter_atom_list(
            variables,
            DEFAULT_ALGEBRAIC_LETTER_POOL_SIZE,
        ))
        .unwrap()
    }

    #[derive(Default)]
    struct ConstantHasher;

    impl Hasher for ConstantHasher {
        fn finish(&self) -> u64 {
            0
        }

        fn write(&mut self, _bytes: &[u8]) {}
    }

    #[test]
    fn accumulator_uses_full_words_under_forced_collisions_and_keeps_order() {
        type ConstantState = BuildHasherDefault<ConstantHasher>;

        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let x_word = Word::new(vec![Rat::parse(ctx.clone(), "x").unwrap()]);
        let y_word = Word::new(vec![Rat::parse(ctx.clone(), "y").unwrap()]);
        let mut rows = Vec::new();
        let mut indices = WordIndex::with_hasher(ConstantState::default());

        bump(
            &mut rows,
            &mut indices,
            x_word.clone(),
            Rat::from_int(ctx.clone(), 2),
        )
        .unwrap();
        bump(
            &mut rows,
            &mut indices,
            y_word.clone(),
            Rat::from_int(ctx.clone(), 5),
        )
        .unwrap();
        bump(
            &mut rows,
            &mut indices,
            x_word.clone(),
            Rat::from_int(ctx.clone(), 3),
        )
        .unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(indices.buckets.len(), 1);
        assert_eq!(indices.buckets.values().next().unwrap().len(), 2);
        assert_eq!(rows[0].word, x_word);
        assert_eq!(rows[1].word, y_word);
        assert_eq!(
            rows[0].coefficient.materialize().unwrap(),
            Rat::from_int(ctx.clone(), 5)
        );
        assert_eq!(
            rows[1].coefficient.materialize().unwrap(),
            Rat::from_int(ctx, 5)
        );
    }

    #[test]
    fn primitive_accumulation_retains_native_rational_coefficients() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let value = Rat::parse(ctx.clone(), "(x+y)/((x+1)^2*(y+1))").unwrap();
        let mut rows = Vec::new();
        let mut indices = WordIndex::default();
        bump(&mut rows, &mut indices, Word::default(), value.clone()).unwrap();
        assert_eq!(rows[0].coefficient.materialize().unwrap(), value);
        assert!(!value.compatibility_views_initialized());
        bump(&mut rows, &mut indices, Word::default(), value.negated()).unwrap();
        assert!(rows[0].coefficient.materialize().unwrap().is_zero());
        assert!(!value.compatibility_views_initialized());
    }

    #[test]
    fn accumulator_does_not_alias_identically_formatted_cross_context_words() {
        type ConstantState = BuildHasherDefault<ConstantHasher>;

        let x_ctx = PolyCtx::new(["x"]).unwrap();
        let y_ctx = PolyCtx::new(["y"]).unwrap();
        let x_word = Word::new(vec![Rat::one(x_ctx.clone())]);
        let y_word = Word::new(vec![Rat::one(y_ctx.clone())]);
        assert_eq!(x_word.content_key(), y_word.content_key());

        let mut rows = Vec::new();
        let mut indices = WordIndex::with_hasher(ConstantState::default());
        bump(&mut rows, &mut indices, x_word, Rat::one(x_ctx)).unwrap();
        bump(&mut rows, &mut indices, y_word, Rat::one(y_ctx)).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(indices.buckets.len(), 1);
    }

    #[test]
    fn integrates_polynomial_and_repeated_pole_terms() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let input = Wordlist::new(vec![WordlistTerm::new(
            Rat::parse(ctx.clone(), "2*x+1/(x+1)^2").unwrap(),
            Word::default(),
        )]);
        let primitive = integrate_ii(&ctx, &input, 0).unwrap();
        assert_eq!(primitive.terms.len(), 1);
        assert_eq!(
            primitive.terms[0].coef,
            Rat::parse(ctx.clone(), "x^2-1/(x+1)").unwrap()
        );
        assert_eq!(differentiate_wordlist(&primitive, 0).unwrap(), input);
    }

    #[test]
    fn sparse_parameter_polynomial_uses_native_symbolica_antiderivative() {
        let ctx = PolyCtx::new(["x", "y"]).unwrap();
        let input = Wordlist::new(vec![WordlistTerm::new(
            Rat::parse(ctx.clone(), "(x^20+3*x^2+1)/(y+1)").unwrap(),
            Word::default(),
        )]);
        let primitive = integrate_ii(&ctx, &input, 0).unwrap();

        assert_eq!(primitive.terms.len(), 1);
        assert_eq!(
            primitive.terms[0].coef,
            Rat::parse(ctx.clone(), "(x^21/21+x^3+x)/(y+1)").unwrap()
        );
        assert_eq!(differentiate_wordlist(&primitive, 0).unwrap(), input);
    }

    #[test]
    fn simple_poles_prepend_letters() {
        let ctx = PolyCtx::new(["x", "a"]).unwrap();
        let input = Wordlist::new(vec![WordlistTerm::new(
            Rat::parse(ctx.clone(), "3/(x-a)").unwrap(),
            Word::new(vec![Rat::from_int(ctx.clone(), 0)]),
        )]);
        let primitive = integrate_ii(&ctx, &input, 0).unwrap();
        assert_eq!(primitive.terms.len(), 1);
        assert_eq!(primitive.terms[0].coef, Rat::from_int(ctx.clone(), 3));
        assert_eq!(
            primitive.terms[0].word,
            Word::new(vec![Rat::parse(ctx.clone(), "a").unwrap(), Rat::zero(ctx)])
        );
    }

    #[test]
    fn factored_prefactor_matches_materialized_primitive_and_derivative() {
        let ctx = PolyCtx::new(["x", "a", "b"]).unwrap();
        let mut prefactor =
            FactoredRat::from_poly(Poly::parse(ctx.clone(), "x^4+a*x^2+b*x+1").unwrap());
        for (base, exponent) in [("2*x-a", 3), ("x+b", 2), ("3*x+1", 1)] {
            prefactor
                .push_factor(&Poly::parse(ctx.clone(), base).unwrap(), exponent)
                .unwrap();
        }
        let unit = Rat::one(ctx.clone());
        let seed = Wordlist::new(vec![WordlistTerm::new(unit.clone(), Word::default())]);

        let actual = integrate_ii_with_factored_prefactor(
            &ctx,
            &seed,
            &prefactor,
            0,
            &IntegrateIiOptions::default(),
        )
        .unwrap();
        assert!(!unit.compatibility_views_initialized());
        assert!(actual.terms.iter().all(|term| {
            !term.coef.compatibility_views_initialized()
                && term
                    .word
                    .letters
                    .iter()
                    .all(|letter| !letter.compatibility_views_initialized())
        }));

        let scaled = Wordlist::new(vec![WordlistTerm::new(
            prefactor.materialize().unwrap(),
            Word::default(),
        )]);
        let expected = integrate_ii(&ctx, &scaled, 0).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(differentiate_wordlist(&actual, 0).unwrap(), scaled);
    }

    #[test]
    fn factored_prefactor_rejects_a_cross_context_wordlist() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let other = PolyCtx::new(["y"]).unwrap();
        let prefactor = FactoredRat::parse(ctx.clone(), "1/(x+1)^2").unwrap();
        let seed = Wordlist::new(vec![WordlistTerm::new(Rat::one(other), Word::default())]);
        let error = integrate_ii_with_factored_prefactor(
            &ctx,
            &seed,
            &prefactor,
            0,
            &IntegrateIiOptions::default(),
        )
        .unwrap_err();
        assert!(matches!(error, Error::ContextMismatch));
    }

    #[test]
    fn nonlinear_denominators_are_typed_failures() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let input = Wordlist::new(vec![WordlistTerm::new(
            Rat::parse(ctx.clone(), "1/(x^2+1)").unwrap(),
            Word::default(),
        )]);
        let error = integrate_ii(&ctx, &input, 0).unwrap_err();
        assert!(error.to_string().contains("IntegrateII"));
        assert!(error.to_string().contains("linear denominator"));
    }

    #[test]
    fn quadratic_poles_are_integrated_as_registered_hyperlog_letters() {
        let _session = begin_algebraic_letter_session().unwrap();
        let ctx = algebraic_context();
        let input = Wordlist::new(vec![WordlistTerm::new(
            Rat::parse(ctx.clone(), "1/(primitive_alg_x^2-primitive_alg_a)").unwrap(),
            Word::default(),
        )]);
        let primitive = integrate_ii_with_options(
            &ctx,
            &input,
            0,
            &IntegrateIiOptions {
                introduce_algebraic_letters: true,
                forbidden_variables: &[],
            },
        )
        .unwrap();

        assert_eq!(primitive.terms.len(), 2);
        assert!(primitive.terms.iter().all(|term| {
            term.word.len() == 1
                && term.word[0].to_atom().as_fun_view().is_some_and(|call| {
                    let heads = crate::symbols::heads();
                    call.get_symbol() == heads.algebraic_minus
                        || call.get_symbol() == heads.algebraic_plus
                })
        }));

        let derivative = differentiate_wordlist(&primitive, 0).unwrap();
        let x = Rat::from_poly(Poly::generator(ctx.clone(), 0).unwrap());
        let split_denominator = x
            .try_sub(&primitive.terms[0].word[0])
            .unwrap()
            .try_mul(&x.try_sub(&primitive.terms[1].word[0]).unwrap())
            .unwrap();
        let expected = Wordlist::new(vec![WordlistTerm::new(
            Rat::one(ctx).try_div(&split_denominator).unwrap(),
            Word::default(),
        )]);
        assert_eq!(derivative, expected);
    }

    #[test]
    fn quadratic_depending_on_later_variable_is_rejected() {
        let _session = begin_algebraic_letter_session().unwrap();
        let ctx = algebraic_context();
        let input = Wordlist::new(vec![WordlistTerm::new(
            Rat::parse(ctx.clone(), "1/(primitive_alg_x^2-primitive_alg_a)").unwrap(),
            Word::default(),
        )]);
        let error = integrate_ii_with_options(
            &ctx,
            &input,
            0,
            &IntegrateIiOptions {
                introduce_algebraic_letters: true,
                forbidden_variables: &[1],
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("remaining integration variable"));
    }
}
