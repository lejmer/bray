use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

use super::ProfileOperation;

thread_local! {
    static OPERATION_DEPTHS: RefCell<Vec<SessionOperationDepths>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug)]
pub(super) struct ProfileConcurrency {
    operation_active_workers: Box<[AtomicU64]>,
    operation_maximum_workers: Box<[AtomicU64]>,
    active_workers: AtomicU64,
    maximum_workers: AtomicU64,
}

impl ProfileConcurrency {
    pub(super) fn new() -> Self {
        let operation_active_workers = (0..ProfileOperation::COUNT)
            .map(|_| AtomicU64::new(0))
            .collect();

        let operation_maximum_workers = (0..ProfileOperation::COUNT)
            .map(|_| AtomicU64::new(0))
            .collect();

        Self {
            operation_active_workers,
            operation_maximum_workers,
            active_workers: AtomicU64::new(0),
            maximum_workers: AtomicU64::new(0),
        }
    }

    pub(super) fn begin_operation(&self, operation: ProfileOperation) {
        if !update_depth(self.identity(), operation, DepthChange::Begin) {
            return;
        }

        let active = self.operation_active_workers[operation.index()]
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);

        self.operation_maximum_workers[operation.index()].fetch_max(active, Ordering::Relaxed);
    }

    pub(super) fn finish_operation(&self, operation: ProfileOperation) {
        if !update_depth(self.identity(), operation, DepthChange::Finish) {
            return;
        }

        self.operation_active_workers[operation.index()].fetch_sub(1, Ordering::Relaxed);
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

    fn identity(&self) -> usize {
        std::ptr::from_ref(self).addr()
    }
}

#[derive(Debug)]
struct SessionOperationDepths {
    session: usize,
    depths: [u64; ProfileOperation::COUNT],
}

#[derive(Clone, Copy)]
enum DepthChange {
    Begin,
    Finish,
}

fn update_depth(session: usize, operation: ProfileOperation, change: DepthChange) -> bool {
    OPERATION_DEPTHS.with_borrow_mut(|sessions| {
        let index = sessions
            .iter()
            .position(|depths| depths.session == session)
            .unwrap_or_else(|| {
                sessions.push(SessionOperationDepths {
                    session,
                    depths: [0; ProfileOperation::COUNT],
                });

                sessions.len() - 1
            });

        let depth = &mut sessions[index].depths[operation.index()];

        let transition = match change {
            DepthChange::Begin => {
                let transition = *depth == 0;

                *depth = depth.saturating_add(1);

                transition
            }
            DepthChange::Finish => {
                let transition = *depth == 1;

                *depth = depth.saturating_sub(1);

                transition
            }
        };

        if sessions[index].depths.iter().all(|depth| *depth == 0) {
            sessions.swap_remove(index);
        }

        transition
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use super::ProfileConcurrency;
    use crate::profile::ProfileOperation;

    #[test]
    fn nested_operations_count_each_thread_once() {
        let concurrency = ProfileConcurrency::new();

        concurrency.begin_operation(ProfileOperation::QueryEvaluation);
        concurrency.begin_operation(ProfileOperation::QueryEvaluation);

        assert_eq!(
            concurrency.operation_maximum_workers(ProfileOperation::QueryEvaluation),
            1
        );

        concurrency.finish_operation(ProfileOperation::QueryEvaluation);
        concurrency.finish_operation(ProfileOperation::QueryEvaluation);
    }

    #[test]
    fn operation_nesting_keeps_distinct_thread_identities() {
        let concurrency = Arc::new(ProfileConcurrency::new());
        let barrier = Arc::new(Barrier::new(3));

        let threads = (0..2)
            .map(|_| {
                let concurrency = Arc::clone(&concurrency);
                let barrier = Arc::clone(&barrier);

                std::thread::spawn(move || {
                    concurrency.begin_operation(ProfileOperation::QueryEvaluation);
                    barrier.wait();
                    barrier.wait();
                    concurrency.finish_operation(ProfileOperation::QueryEvaluation);
                })
            })
            .collect::<Vec<_>>();

        barrier.wait();

        assert_eq!(
            concurrency.operation_maximum_workers(ProfileOperation::QueryEvaluation),
            2
        );

        barrier.wait();

        for thread in threads {
            thread
                .join()
                .unwrap_or_else(|_| panic!("profile worker must finish"));
        }
    }
}
