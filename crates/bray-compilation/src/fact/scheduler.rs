use std::sync::{Mutex, OnceLock};

use rayon::{ThreadPool, ThreadPoolBuilder};

use super::FactQueryError;
use crate::WorkerBudget;

#[derive(Debug)]
pub(crate) struct FactScheduler {
    worker_count: usize,
    pool: OnceLock<Result<ThreadPool, ()>>,
}

impl FactScheduler {
    pub(crate) fn new(worker_budget: WorkerBudget) -> Self {
        Self {
            worker_count: worker_budget.get(),
            pool: OnceLock::new(),
        }
    }

    pub(crate) fn run<T>(&self, operation: impl FnOnce() -> T + Send) -> Result<T, FactQueryError>
    where
        T: Send,
    {
        let pool = self.pool()?;

        if pool.current_thread_index().is_some() {
            Ok(operation())
        } else {
            Ok(pool.install(operation))
        }
    }

    pub(crate) fn map_indexed<T>(
        &self,
        len: usize,
        operation: impl Fn(usize) -> T + Send + Sync,
    ) -> Result<Vec<T>, FactQueryError>
    where
        T: Send,
    {
        let pool = self.pool()?;

        let slots = (0..len)
            .map(|_| Mutex::new(None))
            .collect::<Vec<Mutex<Option<T>>>>();

        pool.scope(|scope| {
            for (index, slot) in slots.iter().enumerate() {
                let operation = &operation;

                scope.spawn(move |_| {
                    let value = operation(index);

                    let mut slot = slot
                        .lock()
                        .unwrap_or_else(|_| panic!("scheduled result slot must remain available"));

                    *slot = Some(value);
                });
            }
        });

        let results = slots
            .into_iter()
            .map(|slot| {
                slot.into_inner()
                    .unwrap_or_else(|_| panic!("scheduled result slot must remain available"))
                    .unwrap_or_else(|| panic!("scheduled work must publish one result"))
            })
            .collect();

        Ok(results)
    }

    #[cfg(test)]
    pub(crate) fn worker_count(&self) -> usize {
        self.worker_count
    }

    fn pool(&self) -> Result<&ThreadPool, FactQueryError> {
        self.pool
            .get_or_init(|| {
                ThreadPoolBuilder::new()
                    .num_threads(self.worker_count)
                    .thread_name(|index| format!("bray-query-{index}"))
                    .build()
                    .map_err(|_| ())
            })
            .as_ref()
            .map_err(|_| FactQueryError::InfrastructureFailure)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::FactScheduler;
    use crate::WorkerBudget;

    #[test]
    fn indexed_work_is_bounded_and_retains_input_order() {
        let scheduler = FactScheduler::new(worker_budget(2));
        let active = AtomicUsize::new(0);
        let maximum = AtomicUsize::new(0);

        let results = scheduler
            .map_indexed(8, |index| {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;

                maximum.fetch_max(current, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));
                active.fetch_sub(1, Ordering::SeqCst);

                index
            })
            .unwrap_or_else(|error| panic!("scheduled work must complete: {error:?}"));

        assert_eq!(results, (0..8).collect::<Vec<_>>());
        assert_eq!(scheduler.worker_count(), 2);
        assert_eq!(maximum.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn serial_nested_work_reuses_the_current_worker() {
        let scheduler = FactScheduler::new(WorkerBudget::serial());

        let (outer, inner) = scheduler
            .run(|| {
                let outer = std::thread::current().id();

                let inner = scheduler
                    .run(|| std::thread::current().id())
                    .unwrap_or_else(|error| panic!("nested work must complete: {error:?}"));

                (outer, inner)
            })
            .unwrap_or_else(|error| panic!("outer work must complete: {error:?}"));

        assert_eq!(outer, inner);
    }

    fn worker_budget(workers: usize) -> WorkerBudget {
        WorkerBudget::new(workers)
            .unwrap_or_else(|error| panic!("test worker budget must be valid: {error:?}"))
    }
}
