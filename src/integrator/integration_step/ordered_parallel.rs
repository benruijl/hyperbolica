use std::iter::Enumerate;
use std::sync::Mutex;

use rayon::ScopeFifo;

/// Admission uses an estimate, not an RSS limit: running entries and native
/// arithmetic scratch may exceed the target. The maximum entry count is hard.
#[derive(Clone, Copy)]
pub(super) struct Lookahead {
    pub minimum: usize,
    pub maximum: usize,
    pub retained_bytes: usize,
}

struct State<I: Iterator, T, A, E> {
    entries: Enumerate<I>,
    ready: Vec<Option<(Result<T, E>, usize)>>,
    next: usize,
    outstanding: usize,
    completed: usize,
    retained_bytes: usize,
    largest_result: usize,
    // Taken by the collecting worker; algebra never holds the metadata lock.
    accumulator: Option<A>,
    error: Option<E>,
}

impl<I: Iterator, T, A, E> State<I, T, A, E> {
    fn reserve(&mut self, limits: Lookahead, grow: bool) -> bool {
        if self.error.is_some() || self.outstanding >= limits.maximum {
            return false;
        }
        let below_floor = self.outstanding < limits.minimum;
        // Ready results already have individual size estimates. Reserve the
        // largest observed size only for unfinished jobs and this new job;
        // charging every ready result at that size needlessly stalls a backlog
        // of small results after one large entry.
        let unfinished = self.outstanding - self.completed;
        let projected = self
            .retained_bytes
            .saturating_add(self.largest_result.saturating_mul(unfinished + 1));
        let within_budget = projected <= limits.retained_bytes;
        if below_floor || (grow && within_budget) {
            self.outstanding += 1;
            true
        } else {
            false
        }
    }
}

struct Pipeline<I: Iterator, T, A, E, P, C, S> {
    state: Mutex<State<I, T, A, E>>,
    process: P,
    collect: C,
    size: S,
    limits: Lookahead,
}

impl<I, T, A, E, P, C, S> Pipeline<I, T, A, E, P, C, S>
where
    I: Iterator + Send,
    I::Item: Send,
    T: Send,
    A: Send,
    E: Send,
    P: Fn(I::Item) -> Result<T, E> + Sync,
    C: Fn(&mut A, T) -> Result<(), E> + Sync,
    S: Fn(&T) -> usize + Sync,
{
    fn run<'scope>(&'scope self, scope: &ScopeFifo<'scope>) {
        let entry = {
            let mut state = self.state.lock().unwrap();
            if state.error.is_some() {
                return;
            }
            let entry = state.entries.next();
            if entry.is_none() {
                state.outstanding -= 1;
            }
            entry
        };
        let Some((index, entry)) = entry else {
            return;
        };
        let result = (self.process)(entry);
        let size = result.as_ref().map(&self.size).unwrap_or(0);
        let mut state = self.state.lock().unwrap();
        if state.error.is_some() {
            return;
        }
        state.retained_bytes = state.retained_bytes.saturating_add(size);
        state.completed += 1;
        state.largest_result = state.largest_result.max(size);
        let slot = index % state.ready.len();
        debug_assert!(state.ready[slot].is_none());
        state.ready[slot] = Some((result, size));
        let next = state.next % state.ready.len();
        if state.accumulator.is_none() || state.ready[next].is_none() {
            // A finished worker may look past a slow earlier entry. Reserve
            // before spawning so queued tasks also count against the bound.
            let start_more = state.reserve(self.limits, true);
            drop(state);
            if start_more {
                scope.spawn_fifo(|scope| self.run(scope));
            }
            return;
        }
        let mut accumulator = state.accumulator.take().unwrap();
        let (mut result, mut size) = state.ready[next].take().unwrap();
        drop(state);
        loop {
            if let Err(error) = result.and_then(|value| (self.collect)(&mut accumulator, value)) {
                self.state.lock().unwrap().error = Some(error);
                return;
            }
            let mut state = self.state.lock().unwrap();
            state.outstanding -= 1;
            state.completed -= 1;
            state.retained_bytes = state.retained_bytes.saturating_sub(size);
            state.next += 1;
            let start_more = state.reserve(self.limits, true);
            let next = state.next % state.ready.len();
            let ready = state.ready[next].take();
            if ready.is_none() {
                state.accumulator = Some(accumulator);
                drop(state);
                if start_more {
                    scope.spawn_fifo(|scope| self.run(scope));
                }
                return;
            }
            (result, size) = ready.unwrap();
            drop(state);
            if start_more {
                scope.spawn_fifo(|scope| self.run(scope));
            }
        }
    }
}

#[cfg(test)]
pub(super) fn try_fold_ordered<I, T, A, E, P, C>(
    entries: I,
    window: usize,
    accumulator: A,
    process: P,
    collect: C,
) -> Result<A, E>
where
    I: Iterator + Send,
    I::Item: Send,
    T: Send,
    A: Send,
    E: Send,
    P: Fn(I::Item) -> Result<T, E> + Sync,
    C: Fn(&mut A, T) -> Result<(), E> + Sync,
{
    try_fold_ordered_adaptive(
        entries,
        Lookahead {
            minimum: window,
            maximum: window,
            retained_bytes: usize::MAX,
        },
        accumulator,
        process,
        collect,
        |_| 0,
    )
}

/// Ordered collection with bounded, memory-aware speculative admission.
/// Small results let idle workers expand beyond the initial lookahead; large
/// results suppress extra admission until the backlog drains. The minimum
/// preserves forward progress even when one result exceeds the byte target.
/// The ring and all admitted/queued tasks remain bounded by `maximum`.
pub(super) fn try_fold_ordered_adaptive<I, T, A, E, P, C, S>(
    entries: I,
    limits: Lookahead,
    accumulator: A,
    process: P,
    collect: C,
    size: S,
) -> Result<A, E>
where
    I: Iterator + Send,
    I::Item: Send,
    T: Send,
    A: Send,
    E: Send,
    P: Fn(I::Item) -> Result<T, E> + Sync,
    C: Fn(&mut A, T) -> Result<(), E> + Sync,
    S: Fn(&T) -> usize + Sync,
{
    assert!(limits.minimum > 0 && limits.maximum >= limits.minimum);
    let pipeline = Pipeline {
        state: Mutex::new(State {
            entries: entries.enumerate(),
            ready: (0..limits.maximum).map(|_| None).collect(),
            next: 0,
            outstanding: limits.minimum,
            completed: 0,
            retained_bytes: 0,
            largest_result: 0,
            accumulator: Some(accumulator),
            error: None,
        }),
        process,
        collect,
        size,
        limits,
    };
    rayon::scope_fifo(|scope| {
        for _ in 0..limits.minimum {
            scope.spawn_fifo(|scope| pipeline.run(scope));
        }
    });
    let state = pipeline.state.into_inner().unwrap();
    match state.error {
        Some(error) => Err(error),
        None => Ok(state.accumulator.unwrap()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Lookahead, State, try_fold_ordered, try_fold_ordered_adaptive};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn adaptive_window_passes_a_slow_first_entry_and_bounds_live_work() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .unwrap();
        let (send, receive) = mpsc::channel();
        let receive = Mutex::new(receive);
        let live = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let output = pool
            .install(|| {
                try_fold_ordered_adaptive(
                    0..40,
                    Lookahead {
                        minimum: 4,
                        maximum: 12,
                        retained_bytes: 1000,
                    },
                    Vec::new(),
                    |i| {
                        let count = live.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(count, Ordering::SeqCst);
                        if i == 0 {
                            receive
                                .lock()
                                .unwrap()
                                .recv_timeout(Duration::from_secs(5))
                                .unwrap();
                        }
                        if i == 8 {
                            send.send(()).unwrap();
                        }
                        Ok::<_, ()>(i)
                    },
                    |output, i| {
                        output.push(i);
                        live.fetch_sub(1, Ordering::SeqCst);
                        Ok(())
                    },
                    |_| 1,
                )
            })
            .unwrap();
        assert_eq!(output, (0..40).collect::<Vec<_>>());
        assert!((9..=12).contains(&peak.load(Ordering::SeqCst)));
        assert_eq!(live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn admission_budget_reserves_queued_tasks_and_preserves_progress_floor() {
        let mut state = State::<_, (), (), ()> {
            entries: (0..10).enumerate(),
            ready: (0..12).map(|_| None).collect(),
            next: 0,
            outstanding: 4,
            completed: 1,
            retained_bytes: 16,
            largest_result: 16,
            accumulator: Some(()),
            error: None,
        };
        let limits = Lookahead {
            minimum: 4,
            maximum: 12,
            retained_bytes: 64,
        };
        // Even with only 16 bytes ready, five reserved results could need 80.
        assert!(!state.reserve(limits, true));
        // Three completed results share 16 bytes; only the unfinished job and
        // the new reservation need the 16-byte worst-observed-size headroom.
        state.completed = 3;
        assert!(state.reserve(limits, true));
        assert_eq!(state.outstanding, 5);
        state.outstanding = 3;
        state.completed = 1;
        state.retained_bytes = 1000;
        assert!(state.reserve(limits, true));
        assert_eq!(state.outstanding, 4);
        assert!(!state.reserve(limits, true));
        state.retained_bytes = 0;
        state.largest_result = 1;
        while state.reserve(limits, true) {}
        assert_eq!(state.outstanding, 12);
    }

    #[test]
    fn adaptive_single_worker_and_ordered_errors_under_small_byte_budget() {
        for workers in [1, 4] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            pool.install(|| {
                for budget in [0, 1000] {
                    let result = try_fold_ordered_adaptive(
                        0..30,
                        Lookahead {
                            minimum: 2,
                            maximum: 16,
                            retained_bytes: budget,
                        },
                        Vec::new(),
                        |i| if i == 7 { Err(i) } else { Ok(i) },
                        |a, i| {
                            if i == 3 {
                                return Err(i);
                            }
                            a.push(i);
                            Ok(())
                        },
                        |_| 100,
                    );
                    assert_eq!(result, Err(3));
                    let result = try_fold_ordered_adaptive(
                        0..30,
                        Lookahead {
                            minimum: 2,
                            maximum: 16,
                            retained_bytes: budget,
                        },
                        0,
                        Ok::<_, ()>,
                        |a, i| {
                            *a += i;
                            Ok(())
                        },
                        |_| 100,
                    );
                    assert_eq!(result, Ok(435));
                }
            });
        }
    }

    #[test]
    fn rolling_window_crosses_old_batch_barrier_and_keeps_order() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .unwrap();
        let (send, receive) = mpsc::channel();
        let receive = Mutex::new(receive);
        let output = pool
            .install(|| {
                try_fold_ordered(
                    0..25,
                    4,
                    Vec::new(),
                    |i| {
                        // The last entry of the old first batch needs an entry
                        // from the next batch to start. A batch barrier fails this.
                        if i == 3 {
                            receive
                                .lock()
                                .unwrap()
                                .recv_timeout(Duration::from_secs(5))
                                .unwrap();
                        } else if i == 4 {
                            send.send(()).unwrap();
                        }
                        Ok::<_, ()>(i)
                    },
                    |output, i| {
                        output.push(i);
                        Ok(())
                    },
                )
            })
            .unwrap();
        assert_eq!(output, (0..25).collect::<Vec<_>>());
    }

    #[test]
    fn collection_overlaps_processing_with_bounded_live_results() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap();
        let live = AtomicUsize::new(0);
        let collected = AtomicUsize::new(0);
        let (send, receive) = mpsc::channel();
        let receive = Mutex::new(receive);
        pool.install(|| {
            try_fold_ordered(
                0..100,
                8,
                (),
                |i| {
                    assert!(live.fetch_add(1, Ordering::SeqCst) < 8);
                    if i == 8 {
                        send.send(()).unwrap();
                    }
                    Ok::<_, ()>(i)
                },
                |(), i| {
                    // Collecting entry 1 must overlap processing entry 8, which
                    // is admitted only after entry 0 has been collected.
                    if i == 1 {
                        receive
                            .lock()
                            .unwrap()
                            .recv_timeout(Duration::from_secs(5))
                            .unwrap();
                    }
                    assert_eq!(collected.fetch_add(1, Ordering::SeqCst), i);
                    live.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                },
            )
        })
        .unwrap();
        assert_eq!(collected.load(Ordering::SeqCst), 100);
    }

    #[test]
    fn nested_single_worker_empty_and_ordered_errors() {
        for threads in [1, 2, 4] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            pool.install(|| {
                assert_eq!(
                    try_fold_ordered(0..0, 2, 7, Ok::<_, usize>, |a, b| {
                        *a += b;
                        Ok(())
                    }),
                    Ok(7)
                );
                for window in [1, 2, 8] {
                    let process_error = try_fold_ordered(
                        0..30,
                        window,
                        Vec::new(),
                        |i| if i == 3 || i == 7 { Err(i) } else { Ok(i) },
                        |a, b| {
                            a.push(b);
                            Ok(())
                        },
                    );
                    assert_eq!(process_error, Err(3));
                    let collect_error = try_fold_ordered(
                        0..30,
                        window,
                        (),
                        |i| if i == 7 { Err(i) } else { Ok(i) },
                        |(), i| if i == 3 { Err(i) } else { Ok(()) },
                    );
                    assert_eq!(collect_error, Err(3));
                    let nested = try_fold_ordered(
                        0..10,
                        window,
                        0,
                        |i| {
                            try_fold_ordered(0..i, window, 0, Ok::<_, ()>, |a, b| {
                                *a += b;
                                Ok(())
                            })
                        },
                        |a, b| {
                            *a += b;
                            Ok(())
                        },
                    );
                    assert_eq!(nested, Ok(120));
                }
            });
        }
    }
}
