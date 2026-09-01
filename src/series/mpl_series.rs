use crate::core::Rat;
use crate::error::Result;
use crate::series::mpl_sum::mpl_sum;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MplSeriesBranch {
    MplSum,
    Unchanged,
    PolyLogDeferred,
    LogSingularity,
    TaylorDeferred,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MplSeriesResult {
    pub branch: MplSeriesBranch,
    pub scalar: Option<Rat>,
}

fn is_free_of_variable(value: &Rat, variable: usize) -> Result<bool> {
    Ok(!value.depends_on(variable)?)
}

fn goes_to_zero(value: &Rat, variable: usize) -> Result<bool> {
    let order = value.pole_degree(variable)?;
    Ok(order == i64::MAX || order > 0)
}

pub fn mpl_series(ns: &[i64], zs: &[Rat], variable: usize, order: i64) -> Result<MplSeriesResult> {
    if ns.len() != zs.len() || ns.is_empty() {
        return Ok(MplSeriesResult {
            branch: MplSeriesBranch::Unchanged,
            scalar: None,
        });
    }

    let mut any_depends = false;
    for z in zs {
        if !is_free_of_variable(z, variable)? {
            any_depends = true;
            break;
        }
    }
    if !any_depends {
        return Ok(MplSeriesResult {
            branch: MplSeriesBranch::Unchanged,
            scalar: None,
        });
    }

    if goes_to_zero(zs.last().expect("non-empty checked above"), variable)?
        || goes_to_zero(&zs[0], variable)?
    {
        return Ok(MplSeriesResult {
            branch: MplSeriesBranch::MplSum,
            scalar: Some(mpl_sum(ns, zs, order)?),
        });
    }

    if ns.len() == 1 {
        return Ok(MplSeriesResult {
            branch: MplSeriesBranch::PolyLogDeferred,
            scalar: None,
        });
    }

    let one = Rat::one(zs[0].ctx().clone());
    if goes_to_zero(
        &zs.last().expect("non-empty checked above").try_sub(&one)?,
        variable,
    )? {
        return Ok(MplSeriesResult {
            branch: MplSeriesBranch::LogSingularity,
            scalar: None,
        });
    }

    Ok(MplSeriesResult {
        branch: MplSeriesBranch::TaylorDeferred,
        scalar: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::PolyCtx;

    #[test]
    fn dispatcher_covers_defining_sum_and_deferred_branches() {
        let ctx = PolyCtx::new(["x"]).unwrap();
        let x = Rat::parse(ctx.clone(), "x").unwrap();
        let sum = mpl_series(&[1], std::slice::from_ref(&x), 0, 2).unwrap();
        assert_eq!(sum.branch, MplSeriesBranch::MplSum);
        assert_eq!(
            sum.scalar.unwrap(),
            Rat::parse(ctx.clone(), "x+x^2/2").unwrap()
        );

        let one_plus_x = Rat::parse(ctx.clone(), "1+x").unwrap();
        assert_eq!(
            mpl_series(&[2], std::slice::from_ref(&one_plus_x), 0, 2)
                .unwrap()
                .branch,
            MplSeriesBranch::PolyLogDeferred
        );

        let two_plus_x = Rat::parse(ctx.clone(), "2+x").unwrap();
        assert_eq!(
            mpl_series(&[1, 1], &[two_plus_x.clone(), one_plus_x.clone()], 0, 2)
                .unwrap()
                .branch,
            MplSeriesBranch::LogSingularity
        );
        assert!(!x.compatibility_views_initialized());
        assert!(!one_plus_x.compatibility_views_initialized());
        assert!(!two_plus_x.compatibility_views_initialized());
    }
}
