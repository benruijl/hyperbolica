use std::collections::HashMap;

use crate::core::Poly;
use crate::error::{Error, Result};

use super::super::lr_find_roots::{PathState, step_fr_judge};
use super::super::lr_reduction::leaf_count_proxy;
use super::LrResult;

pub(super) fn carry_discharge_result(
    table: &HashMap<u64, Vec<Vec<Poly>>>,
    variables: &[usize],
) -> Result<LrResult> {
    let mut driver = CarryDriver {
        table,
        variables,
        best: None,
    };
    driver.visit(
        0,
        &mut Vec::new(),
        0.0,
        &mut Vec::new(),
        PathState::default(),
    )?;
    Ok(driver
        .best
        .map(|best| LrResult {
            order: best.order,
            score: best.score,
            root_polys: best.roots,
            carried_sqrts: best.state.nsq,
            kin_sqrts: best.state.nkin,
            terminal_quads: best.state.ntq,
            obligation_polys: best.state.minted,
        })
        .unwrap_or_else(|| LrResult::empty(f64::INFINITY)))
}

#[derive(Clone)]
struct CarryBest {
    order: Vec<usize>,
    score: f64,
    roots: Vec<Poly>,
    state: PathState,
}

struct CarryDriver<'a> {
    table: &'a HashMap<u64, Vec<Vec<Poly>>>,
    variables: &'a [usize],
    best: Option<CarryBest>,
}

impl CarryDriver<'_> {
    fn key_beats(
        nsq: u64,
        nonexec: bool,
        score: f64,
        other_nsq: u64,
        other_nonexec: bool,
        other_score: f64,
    ) -> bool {
        (nsq, nonexec, score) < (other_nsq, other_nonexec, other_score)
    }

    fn visit(
        &mut self,
        bits: u64,
        order: &mut Vec<usize>,
        score: f64,
        roots: &mut Vec<Poly>,
        state: PathState,
    ) -> Result<()> {
        if order.len() == self.variables.len() {
            let replace = self.best.as_ref().is_none_or(|best| {
                Self::key_beats(
                    state.nsq,
                    state.nonexec,
                    score,
                    best.state.nsq,
                    best.state.nonexec,
                    best.score,
                )
            });
            if replace {
                self.best = Some(CarryBest {
                    order: order.clone(),
                    score,
                    roots: roots.clone(),
                    state,
                });
            }
            return Ok(());
        }
        if let Some(best) = &self.best
            && !Self::key_beats(
                state.nsq,
                state.nonexec,
                score,
                best.state.nsq,
                best.state.nonexec,
                best.score,
            )
        {
            return Ok(());
        }

        for bit in 0..self.variables.len() {
            if bits & (1_u64 << bit) != 0 {
                continue;
            }
            let pivot = self.variables[bit];
            let pending = (0..self.variables.len())
                .filter(|next| *next != bit && bits & (1_u64 << next) == 0)
                .map(|next| self.variables[next])
                .collect::<Vec<_>>();
            let (letters, extension, step_roots) = {
                let parent = self.table.get(&bits).ok_or_else(|| {
                    Error::InvalidInput("missing carry-discharge subset state".into())
                })?;
                let mut letters = Vec::new();
                let mut extension = score;
                let mut step_roots = Vec::new();
                for group in parent {
                    extension += (leaf_count_proxy(group) as f64).powf(1.15);
                    for letter in group {
                        if letter.degree(pivot)? == 2 {
                            step_roots.push(letter.clone());
                        }
                        letters.push(letter.clone());
                    }
                }
                (letters, extension, step_roots)
            };
            let mut next_state = state.clone();
            if !step_fr_judge(
                &letters,
                pivot,
                None,
                &pending,
                self.variables,
                &mut next_state,
            )? {
                continue;
            }

            order.push(pivot);
            let root_mark = roots.len();
            roots.extend(step_roots);
            self.visit(bits | (1_u64 << bit), order, extension, roots, next_state)?;
            roots.truncate(root_mark);
            order.pop();
        }
        Ok(())
    }
}
