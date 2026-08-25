use std::collections::BTreeSet;

use super::{CancellationToken, FactQueryError, FactRuntime};

/// One completed item and the additional work that it requires.
#[derive(Debug)]
pub(crate) struct BatchWork<K, T> {
    value: T,
    required: Vec<K>,
}

impl<K, T> BatchWork<K, T> {
    pub(crate) fn new(value: T, required: impl IntoIterator<Item = K>) -> Self {
        Self {
            value,
            required: required.into_iter().collect(),
        }
    }

    pub(crate) fn leaf(value: T) -> Self {
        Self {
            value,
            required: Vec::new(),
        }
    }
}

/// Failure to complete one deterministic batch closure.
#[derive(Debug)]
pub(crate) enum BatchCompletionError<K, E> {
    Cancelled,
    Evaluation { key: K, error: E },
    Scheduler(FactQueryError),
}

impl<K> BatchCompletionError<K, FactQueryError> {
    pub(crate) fn into_fact_query_error(self) -> FactQueryError {
        match self {
            Self::Cancelled => FactQueryError::Cancelled,
            Self::Evaluation { error, .. } | Self::Scheduler(error) => error,
        }
    }
}

impl FactRuntime {
    /// Completes a dynamically discovered work closure in deterministic bounded waves.
    ///
    /// Root and required-work iteration order define plan order and must be stable. Each wave
    /// finishes before its newly required work becomes eligible. Narrow evaluator requests
    /// continue through the ordinary fact-query path.
    pub(crate) fn complete_batch<K, T, E>(
        &self,
        roots: impl IntoIterator<Item = K>,
        cancellation: &CancellationToken,
        evaluator: impl Fn(&K) -> Result<BatchWork<K, T>, E> + Send + Sync,
    ) -> Result<Vec<(K, T)>, BatchCompletionError<K, E>>
    where
        K: Clone + Ord + Send + Sync,
        T: Send,
        E: Send,
    {
        let mut plan = BatchPlan::new(roots);

        while let Some(wave) = plan.take_wave() {
            if cancellation.is_cancelled() {
                return Err(BatchCompletionError::Cancelled);
            }

            let outcomes = match self.map_indexed(wave.len(), |index| {
                if cancellation.is_cancelled() {
                    None
                } else {
                    Some(evaluator(&wave[index]))
                }
            }) {
                Ok(outcomes) => outcomes,
                Err(FactQueryError::Cancelled) => {
                    return Err(BatchCompletionError::Cancelled);
                }
                Err(error) => return Err(BatchCompletionError::Scheduler(error)),
            };

            let mut completed = Vec::with_capacity(wave.len());

            for (key, outcome) in wave.into_iter().zip(outcomes) {
                let Some(outcome) = outcome else {
                    return Err(BatchCompletionError::Cancelled);
                };

                let work = match outcome {
                    Ok(work) => work,
                    Err(error) => return Err(BatchCompletionError::Evaluation { key, error }),
                };

                completed.push((key, work));
            }

            if cancellation.is_cancelled() {
                return Err(BatchCompletionError::Cancelled);
            }

            plan.complete_wave(completed);
        }

        Ok(plan.finish())
    }
}

#[derive(Debug)]
struct BatchPlan<K, T> {
    seen: BTreeSet<K>,
    ready: Vec<K>,
    completed: Vec<(K, T)>,
}

impl<K, T> BatchPlan<K, T>
where
    K: Clone + Ord,
{
    fn new(roots: impl IntoIterator<Item = K>) -> Self {
        let mut plan = Self {
            seen: BTreeSet::new(),
            ready: Vec::new(),
            completed: Vec::new(),
        };

        for root in roots {
            plan.schedule(root);
        }

        plan
    }

    fn take_wave(&mut self) -> Option<Vec<K>> {
        if self.ready.is_empty() {
            return None;
        }

        Some(std::mem::take(&mut self.ready))
    }

    fn complete_wave(&mut self, completed: Vec<(K, BatchWork<K, T>)>) {
        for (key, work) in completed {
            self.completed.push((key, work.value));

            for key in work.required {
                self.schedule(key);
            }
        }
    }

    fn finish(self) -> Vec<(K, T)> {
        self.completed
    }

    fn schedule(&mut self, key: K) {
        // The deduplication index and ready queue independently own each stable work identity.
        if self.seen.insert(key.clone()) {
            self.ready.push(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    use super::{BatchCompletionError, BatchWork};
    use crate::fact::{CancellationToken, FactRuntime};
    use crate::{QueryPriority, WorkerBudget};

    #[test]
    fn serial_and_parallel_closures_are_deduplicated_and_ordered() {
        let serial = completion(WorkerBudget::serial());

        let parallel_budget = WorkerBudget::new(4)
            .unwrap_or_else(|error| panic!("parallel worker budget must be valid: {error:?}"));

        let parallel = completion(parallel_budget);

        assert_eq!(serial, parallel);

        assert_eq!(
            serial,
            [(1, 10), (3, 30), (2, 20), (4, 40), (5, 50), (6, 60)]
        );
    }

    #[test]
    fn work_never_exceeds_the_runtime_worker_budget() {
        let budget = WorkerBudget::new(4)
            .unwrap_or_else(|error| panic!("parallel worker budget must be valid: {error:?}"));

        let runtime = FactRuntime::new(budget);
        let cancellation = CancellationToken::new();
        let active = AtomicUsize::new(0);
        let maximum = AtomicUsize::new(0);

        let result = runtime.complete_batch(0..32, &cancellation, |key| {
            let current = active.fetch_add(1, Ordering::SeqCst) + 1;

            maximum.fetch_max(current, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(2));
            active.fetch_sub(1, Ordering::SeqCst);

            Ok::<_, ()>(BatchWork::leaf(*key))
        });

        assert!(result.is_ok());
        assert!(maximum.load(Ordering::SeqCst) > 1);
        assert!(maximum.load(Ordering::SeqCst) <= budget.get());
    }

    #[test]
    fn batch_work_retains_the_request_priority() {
        let runtime = FactRuntime::new(WorkerBudget::serial());
        let cancellation = CancellationToken::new();

        let completed = runtime.run(QueryPriority::Interactive, || {
            runtime
                .complete_batch([1], &cancellation, |_| {
                    runtime.current_priority().map(BatchWork::<u32, _>::leaf)
                })
                .map_err(BatchCompletionError::into_fact_query_error)
        });

        assert_eq!(completed, Ok(vec![(1, Some(QueryPriority::Interactive))]));
    }

    #[test]
    fn evaluator_panics_remain_invariant_failures() {
        let runtime = FactRuntime::new(WorkerBudget::serial());
        let cancellation = CancellationToken::new();

        let outcome = catch_unwind(AssertUnwindSafe(|| {
            runtime.complete_batch([1], &cancellation, |_| -> Result<BatchWork<u32, ()>, ()> {
                panic!("test evaluator invariant failed")
            })
        }));

        assert!(outcome.is_err());
    }

    #[test]
    fn cancellation_discards_the_current_wave_and_undispatched_work() {
        let runtime = FactRuntime::new(WorkerBudget::serial());
        let cancellation = CancellationToken::new();
        let visited = Mutex::new(Vec::new());

        let result = runtime.complete_batch([1], &cancellation, |key| {
            visited
                .lock()
                .unwrap_or_else(|_| panic!("visited work must remain available"))
                .push(*key);

            cancellation.cancel();

            Ok::<_, ()>(BatchWork::new(*key, [2]))
        });

        assert!(matches!(result, Err(BatchCompletionError::Cancelled)));

        assert_eq!(
            *visited
                .lock()
                .unwrap_or_else(|_| panic!("visited work must remain available")),
            [1]
        );
    }

    #[test]
    fn parallel_failure_selection_follows_plan_order() {
        let budget = WorkerBudget::new(2)
            .unwrap_or_else(|error| panic!("parallel worker budget must be valid: {error:?}"));

        let runtime = FactRuntime::new(budget);
        let cancellation = CancellationToken::new();
        let later_finished = Arc::new((Mutex::new(false), Condvar::new()));

        let result = runtime.complete_batch([1, 2], &cancellation, {
            let later_finished = Arc::clone(&later_finished);

            move |key| -> Result<BatchWork<u32, ()>, u32> {
                if *key == 1 {
                    let (lock, completed) = &*later_finished;

                    let finished = lock
                        .lock()
                        .unwrap_or_else(|_| panic!("completion state must remain available"));

                    let _finished = completed
                        .wait_while(finished, |finished| !*finished)
                        .unwrap_or_else(|_| panic!("completion state must remain available"));
                } else {
                    let (lock, completed) = &*later_finished;

                    let mut finished = lock
                        .lock()
                        .unwrap_or_else(|_| panic!("completion state must remain available"));

                    *finished = true;
                    completed.notify_all();
                }

                Err(*key)
            }
        });

        assert!(matches!(
            result,
            Err(BatchCompletionError::Evaluation { key: 1, error: 1 })
        ));
    }

    fn completion(worker_budget: WorkerBudget) -> Vec<(u32, u32)> {
        let runtime = FactRuntime::new(worker_budget);
        let cancellation = CancellationToken::new();

        runtime
            .complete_batch([1, 3, 1], &cancellation, |key| {
                let required = match *key {
                    1 => vec![2, 4],
                    2 => vec![5],
                    3 => vec![2, 5],
                    4 => vec![6, 1],
                    5 => vec![6],
                    _ => Vec::new(),
                };

                Ok::<_, ()>(BatchWork::new(*key * 10, required))
            })
            .unwrap_or_else(|error| panic!("batch closure must complete: {error:?}"))
    }
}
