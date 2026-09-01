use crate::core::Rat;
use crate::error::Result;
use crate::series::expansions::expand_zero_word;
use crate::symbols::Word;

/// One term `coef * log(arg)^log_power / log_power! * arg^arg_power`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpansionTerm {
    pub log_power: i64,
    pub arg_power: i64,
    pub coef: Rat,
}

pub type ExpansionSeries = Vec<ExpansionTerm>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HlogSeriesBranch {
    ZeroLimit,
    Unchanged,
    TaylorDeferred,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HlogSeriesResult {
    pub branch: HlogSeriesBranch,
    pub terms: ExpansionSeries,
}

fn is_free_of_variable(value: &Rat, variable: usize) -> Result<bool> {
    Ok(!value.depends_on(variable)?)
}

fn goes_to_zero(value: &Rat, variable: usize) -> Result<bool> {
    let order = value.pole_degree(variable)?;
    Ok(order == i64::MAX || order > 0)
}

/// Expand `Hlog(arg, word)` in powers and logarithms of an argument tending
/// to zero.  The argument supplies the context; the coefficients depend only
/// on the word, as in the Panzer/Brown recurrence used upstream.
pub fn hlog_zero_expand(arg: &Rat, word: &Word, order: i64) -> Result<ExpansionSeries> {
    if word.is_empty() {
        return Ok(vec![ExpansionTerm {
            log_power: 0,
            arg_power: 0,
            coef: Rat::one(arg.ctx().clone()),
        }]);
    }

    let table = expand_zero_word(word, order)?;
    let mut terms = Vec::new();
    for (log_power, row) in table.into_iter().enumerate() {
        for (arg_power, coef) in row.into_iter().enumerate() {
            if !coef.is_zero() {
                terms.push(ExpansionTerm {
                    log_power: log_power as i64,
                    arg_power: arg_power as i64,
                    coef,
                });
            }
        }
    }
    Ok(terms)
}

pub fn hlog_series(
    arg: &Rat,
    word: &Word,
    variable: usize,
    order: i64,
) -> Result<HlogSeriesResult> {
    let mut word_has_variable = false;
    for letter in &word.letters {
        if !is_free_of_variable(letter, variable)? {
            word_has_variable = true;
            break;
        }
    }

    if is_free_of_variable(arg, variable)? && !word_has_variable {
        return Ok(HlogSeriesResult {
            branch: HlogSeriesBranch::Unchanged,
            terms: Vec::new(),
        });
    }
    if goes_to_zero(arg, variable)? {
        return Ok(HlogSeriesResult {
            branch: HlogSeriesBranch::ZeroLimit,
            terms: hlog_zero_expand(arg, word, order)?,
        });
    }
    Ok(HlogSeriesResult {
        branch: HlogSeriesBranch::TaylorDeferred,
        terms: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::PolyCtx;

    #[test]
    fn empty_word_expands_to_one() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let x = Rat::parse(ctx, "x").unwrap();
        let terms = hlog_zero_expand(&x, &Word::default(), 3).unwrap();
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].log_power, 0);
        assert_eq!(terms[0].arg_power, 0);
        assert!(terms[0].coef.is_one());
    }

    #[test]
    fn dispatcher_distinguishes_constant_zero_and_generic_limits() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let zero_letter = Rat::zero(ctx.clone());
        let word = Word::new(vec![zero_letter]);
        let constant = Rat::from_int(ctx.clone(), 2);
        assert_eq!(
            hlog_series(&constant, &Word::default(), 0, 2)
                .unwrap()
                .branch,
            HlogSeriesBranch::Unchanged
        );

        let x = Rat::parse(ctx.clone(), "x").unwrap();
        assert_eq!(
            hlog_series(&x, &word, 0, 2).unwrap().branch,
            HlogSeriesBranch::ZeroLimit
        );

        let one_plus_x = Rat::parse(ctx, "1+x").unwrap();
        assert_eq!(
            hlog_series(&one_plus_x, &word, 0, 2).unwrap().branch,
            HlogSeriesBranch::TaylorDeferred
        );
        assert!(!constant.compatibility_views_initialized());
        assert!(!x.compatibility_views_initialized());
        assert!(!one_plus_x.compatibility_views_initialized());
        assert!(!word.letters[0].compatibility_views_initialized());
    }
}
