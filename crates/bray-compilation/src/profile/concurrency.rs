use std::sync::atomic::{AtomicU64, Ordering};

use super::ProfileOperation;

#[derive(Debug)]
pub(super) struct ProfileConcurrency {
    worker_count: usize,
    operation_depths: Box<[AtomicU64]>,
    operation_active_workers: Box<[AtomicU64]>,
    operation_maximum_workers: Box<[AtomicU64]>,
    active_workers: AtomicU64,
    maximum_workers: AtomicU64,
}

impl ProfileConcurrency {
    pub(super) fn new(worker_count: usize) -> Self {
        let operation_depths = (0..ProfileOperation::COUNT.saturating_mul(worker_count))
            .map(|_| AtomicU64::new(0))
            .collect();

        let operation_active_workers = (0..ProfileOperation::COUNT)
            .map(|_| AtomicU64::new(0))
            .collect();

        let operation_maximum_workers = (0..ProfileOperation::COUNT)
            .map(|_| AtomicU64::new(0))
            .collect();

        Self {
            worker_count,
            operation_depths,
            operation_active_workers,
            operation_maximum_workers,
            active_workers: AtomicU64::new(0),
            maximum_workers: AtomicU64::new(0),
        }
    }

    pub(super) fn begin_operation(&self, operation: ProfileOperation, worker: usize) {
        let depth = &self.operation_depths[self.operation_worker_index(operation, worker)];

        if depth.fetch_add(1, Ordering::Relaxed) > 0 {
            return;
        }

        let active = self.operation_active_workers[operation.index()]
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);

        self.operation_maximum_workers[operation.index()].fetch_max(active, Ordering::Relaxed);
    }

    pub(super) fn finish_operation(&self, operation: ProfileOperation, worker: usize) {
        let depth = &self.operation_depths[self.operation_worker_index(operation, worker)];

        if depth.fetch_sub(1, Ordering::Relaxed) == 1 {
            self.operation_active_workers[operation.index()].fetch_sub(1, Ordering::Relaxed);
        }
    }

    pub(super) fn operation_maximum_workers(&self, operation: ProfileOperation) -> u64 {
        self.operation_maximum_workers[operation.index()].load(Ordering::Relaxed)
    }

    pub(super) fn begin_worker(&self) {
        let active = self
            .active_workers
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);

        self.maximum_workers.fetch_max(active, Ordering::Relaxed);
    }

    pub(super) fn finish_worker(&self) {
        self.active_workers.fetch_sub(1, Ordering::Relaxed);
    }

    pub(super) fn maximum_workers(&self) -> u64 {
        self.maximum_workers.load(Ordering::Relaxed)
    }

    fn operation_worker_index(&self, operation: ProfileOperation, worker: usize) -> usize {
        operation
            .index()
            .saturating_mul(self.worker_count)
            .saturating_add(worker)
    }
}

#[cfg(test)]
mod tests {
    use super::ProfileConcurrency;
    use crate::profile::ProfileOperation;

    #[test]
    fn nested_operations_count_each_worker_once() {
        let concurrency = ProfileConcurrency::new(2);

        concurrency.begin_operation(ProfileOperation::QueryEvaluation, 0);
        concurrency.begin_operation(ProfileOperation::QueryEvaluation, 0);
        concurrency.begin_operation(ProfileOperation::QueryEvaluation, 1);

        assert_eq!(
            concurrency.operation_maximum_workers(ProfileOperation::QueryEvaluation),
            2
        );

        concurrency.finish_operation(ProfileOperation::QueryEvaluation, 0);
        concurrency.finish_operation(ProfileOperation::QueryEvaluation, 0);
        concurrency.finish_operation(ProfileOperation::QueryEvaluation, 1);
    }
}
