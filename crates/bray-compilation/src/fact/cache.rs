use std::sync::{Condvar, Mutex, OnceLock};
use std::thread::{self, ThreadId};
use std::time::Duration;

use super::{CancellationToken, CompilationFactKey, FactQueryError, FactRuntime};

const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub(crate) struct FactCell<T> {
    value: OnceLock<T>,
    state: Mutex<FactCellState>,
    changed: Condvar,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FactCellState {
    Vacant,
    Computing {
        owner: ThreadId,
        key: CompilationFactKey,
    },
    Ready(CompilationFactKey),
}

impl<T> FactCell<T> {
    pub(crate) fn new() -> Self {
        Self {
            value: OnceLock::new(),
            state: Mutex::new(FactCellState::Vacant),
            changed: Condvar::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn get(&self) -> Option<&T> {
        self.value.get()
    }

    pub(crate) fn get_or_compute(
        &self,
        runtime: &FactRuntime,
        key: CompilationFactKey,
        cancellation: &CancellationToken,
        compute: impl FnOnce() -> Result<T, FactQueryError>,
    ) -> Result<&T, FactQueryError> {
        let mut compute = Some(compute);

        loop {
            cancellation.check()?;

            let mut state = self
                .state
                .lock()
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            match *state {
                FactCellState::Ready(ready_key) => {
                    if ready_key != key {
                        return Err(FactQueryError::InfrastructureFailure);
                    }

                    return self.ready_value();
                }
                FactCellState::Vacant => {
                    let thread = thread::current().id();
                    *state = FactCellState::Computing { owner: thread, key };

                    drop(state);

                    let mut publication = PublicationGuard::new(self, thread, key);

                    let _evaluation = runtime.begin(key)?;

                    cancellation.check()?;

                    let compute = compute
                        .take()
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    let value = compute()?;

                    cancellation.check()?;

                    self.publish(value, thread, key)?;

                    publication.disarm();

                    return self.ready_value();
                }
                FactCellState::Computing {
                    owner,
                    key: computing_key,
                } => {
                    if computing_key != key {
                        return Err(FactQueryError::InfrastructureFailure);
                    }

                    let thread = thread::current().id();

                    if owner == thread {
                        return Err(FactQueryError::Cycle(runtime.same_thread_cycle(key)?));
                    }

                    let waiting = runtime.wait_for(key, owner)?;

                    let waited = self
                        .changed
                        .wait_timeout(state, CANCELLATION_POLL_INTERVAL)
                        .map_err(|_| FactQueryError::InfrastructureFailure)?;

                    drop(waiting);
                    drop(waited);
                }
            }
        }
    }

    fn publish(
        &self,
        value: T,
        thread: ThreadId,
        key: CompilationFactKey,
    ) -> Result<(), FactQueryError> {
        self.value
            .set(value)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let mut state = self
            .state
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        if *state != (FactCellState::Computing { owner: thread, key }) {
            return Err(FactQueryError::InfrastructureFailure);
        }

        *state = FactCellState::Ready(key);

        self.changed.notify_all();

        Ok(())
    }

    fn ready_value(&self) -> Result<&T, FactQueryError> {
        self.value
            .get()
            .ok_or(FactQueryError::InfrastructureFailure)
    }

    fn abandon(&self, thread: ThreadId, key: CompilationFactKey) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        if *state != (FactCellState::Computing { owner: thread, key }) {
            return;
        }

        *state = if self.value.get().is_some() {
            FactCellState::Ready(key)
        } else {
            FactCellState::Vacant
        };

        self.changed.notify_all();
    }
}

impl<T> Default for FactCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

struct PublicationGuard<'a, T> {
    cell: &'a FactCell<T>,
    thread: ThreadId,
    key: CompilationFactKey,
    active: bool,
}

impl<'a, T> PublicationGuard<'a, T> {
    fn new(cell: &'a FactCell<T>, thread: ThreadId, key: CompilationFactKey) -> Self {
        Self {
            cell,
            thread,
            key,
            active: true,
        }
    }

    fn disarm(&mut self) {
        self.active = false;
    }
}

impl<T> Drop for PublicationGuard<'_, T> {
    fn drop(&mut self) {
        if self.active {
            self.cell.abandon(self.thread, self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    use std::time::Duration;

    use bray_diagnostics::{
        Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticResult, SeverityKind,
    };

    use super::FactCell;
    use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, FactRuntime};

    #[test]
    fn concurrent_requests_compute_one_value() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();

        let cell = FactCell::new();
        let computations = AtomicUsize::new(0);

        std::thread::scope(|scope| {
            let handles = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        cell.get_or_compute(
                            &runtime,
                            CompilationFactKey::SyntaxTree,
                            &cancellation,
                            || {
                                computations.fetch_add(1, Ordering::SeqCst);
                                std::thread::sleep(Duration::from_millis(20));

                                Ok(42_u32)
                            },
                        )
                        .copied()
                    })
                })
                .collect::<Vec<_>>();

            for handle in handles {
                let result = join(handle);

                assert_eq!(result, Ok(42));
            }
        });

        assert_eq!(computations.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn independent_facts_compute_concurrently() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();

        let first = FactCell::new();
        let second = FactCell::new();

        let barrier = Barrier::new(2);

        let results = std::thread::scope(|scope| {
            let first_handle = scope.spawn(|| {
                first
                    .get_or_compute(
                        &runtime,
                        CompilationFactKey::SyntaxTree,
                        &cancellation,
                        || {
                            barrier.wait();

                            Ok(1_u32)
                        },
                    )
                    .copied()
            });

            let second_handle = scope.spawn(|| {
                second
                    .get_or_compute(
                        &runtime,
                        CompilationFactKey::DeclarationTable,
                        &cancellation,
                        || {
                            barrier.wait();

                            Ok(2_u32)
                        },
                    )
                    .copied()
            });

            [join(first_handle), join(second_handle)]
        });

        assert_eq!(results, [Ok(1), Ok(2)]);
    }

    #[test]
    fn same_thread_cycles_are_reported_before_waiting() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cell = FactCell::new();

        let key = CompilationFactKey::SyntaxTree;

        let result = cell.get_or_compute(&runtime, key, &cancellation, || {
            cell.get_or_compute(&runtime, key, &cancellation, || Ok(2_u32))
                .copied()
        });

        let error = match result {
            Ok(_) => panic!("recursive fact request should report a cycle"),
            Err(error) => error,
        };

        let FactQueryError::Cycle(cycle) = error else {
            panic!("recursive fact request should report a cycle");
        };

        assert_eq!(cycle.facts(), &[key, key]);
        assert!(cell.get().is_none());
    }

    #[test]
    fn cache_cells_reject_reuse_for_another_fact_key() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cell = FactCell::new();

        let first = cell.get_or_compute(
            &runtime,
            CompilationFactKey::SyntaxTree,
            &cancellation,
            || Ok(1_u32),
        );

        let mismatched = cell.get_or_compute(
            &runtime,
            CompilationFactKey::DeclarationTable,
            &cancellation,
            || Ok(2_u32),
        );

        assert_eq!(first, Ok(&1));
        assert_eq!(mismatched, Err(FactQueryError::InfrastructureFailure));
        assert_eq!(cell.get(), Some(&1));
    }

    #[test]
    fn cross_thread_cycles_do_not_deadlock() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();

        let first = FactCell::new();
        let second = FactCell::new();

        let barrier = Barrier::new(2);

        let first_key = CompilationFactKey::SyntaxTree;
        let second_key = CompilationFactKey::DeclarationTable;

        let (first_result, second_result) = std::thread::scope(|scope| {
            let first_handle = scope.spawn(|| {
                first
                    .get_or_compute(&runtime, first_key, &cancellation, || {
                        barrier.wait();

                        second
                            .get_or_compute(&runtime, second_key, &cancellation, || Ok(20_u32))
                            .copied()
                    })
                    .copied()
            });

            let second_handle = scope.spawn(|| {
                second
                    .get_or_compute(&runtime, second_key, &cancellation, || {
                        barrier.wait();

                        first
                            .get_or_compute(&runtime, first_key, &cancellation, || Ok(10_u32))
                            .copied()
                    })
                    .copied()
            });

            (join(first_handle), join(second_handle))
        });

        let results = [first_result, second_result];

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(FactQueryError::Cycle(_))))
                .count(),
            1
        );
    }

    #[test]
    fn cancellation_publishes_nothing() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cell = FactCell::new();

        let key = CompilationFactKey::SyntaxTree;

        let result = cell.get_or_compute(&runtime, key, &cancellation, || {
            cancellation.cancel();

            Ok(7_u32)
        });

        assert_eq!(result, Err(FactQueryError::Cancelled));
        assert!(cell.get().is_none());

        let result = cell.get_or_compute(&runtime, key, &CancellationToken::new(), || Ok(9_u32));

        assert_eq!(result, Ok(&9));
    }

    #[test]
    fn waiting_requests_observe_cancellation() {
        let runtime = FactRuntime::default();

        let owner_cancellation = CancellationToken::new();
        let waiter_cancellation = CancellationToken::new();

        let cell = FactCell::new();
        let key = CompilationFactKey::SyntaxTree;

        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let (result_sender, result_receiver) = mpsc::channel();

        let observed = std::thread::scope(|scope| {
            let owner_cell = &cell;
            let owner_runtime = &runtime;
            let owner_token = &owner_cancellation;

            let owner = scope.spawn(move || {
                owner_cell
                    .get_or_compute(owner_runtime, key, owner_token, || {
                        if started_sender.send(()).is_err() {
                            panic!("test should observe the computing fact");
                        }

                        if release_receiver.recv().is_err() {
                            panic!("test should release the computing fact");
                        }

                        Ok(5_u32)
                    })
                    .copied()
            });

            if started_receiver.recv().is_err() {
                panic!("owner fact should start computing");
            }

            let waiter_cell = &cell;
            let waiter_runtime = &runtime;
            let waiter_token = &waiter_cancellation;

            let waiter = scope.spawn(move || {
                let result = waiter_cell
                    .get_or_compute(waiter_runtime, key, waiter_token, || Ok(9_u32))
                    .copied();

                if result_sender.send(result).is_err() {
                    panic!("test should observe the waiting result");
                }
            });

            std::thread::sleep(Duration::from_millis(20));

            waiter_cancellation.cancel();

            let observed = result_receiver.recv_timeout(Duration::from_secs(1));

            if release_sender.send(()).is_err() {
                panic!("test should release the owner fact");
            }

            let owner_result = join(owner);

            if waiter.join().is_err() {
                panic!("waiting fact thread should not panic");
            }

            assert_eq!(owner_result, Ok(5));

            observed
        });

        assert_eq!(observed, Ok(Err(FactQueryError::Cancelled)));
        assert_eq!(cell.get(), Some(&5));
    }

    #[test]
    fn values_and_diagnostics_publish_atomically() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cell = FactCell::new();
        let computations = AtomicUsize::new(0);

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(3),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Error,
        );

        let first = match cell.get_or_compute(
            &runtime,
            CompilationFactKey::DeclarationTable,
            &cancellation,
            || {
                computations.fetch_add(1, Ordering::SeqCst);

                Ok(DiagnosticResult::new(
                    11_u32,
                    DiagnosticBag::single(diagnostic.clone()),
                ))
            },
        ) {
            Ok(result) => result,
            Err(error) => panic!("diagnostic fact should publish: {error:?}"),
        };

        let second = match cell.get_or_compute(
            &runtime,
            CompilationFactKey::DeclarationTable,
            &cancellation,
            || {
                computations.fetch_add(1, Ordering::SeqCst);

                Ok(DiagnosticResult::without_diagnostics(99_u32))
            },
        ) {
            Ok(result) => result,
            Err(error) => panic!("cached diagnostic fact should be available: {error:?}"),
        };

        assert!(std::ptr::eq(first, second));
        assert_eq!(first.value(), &11);
        assert_eq!(first.diagnostics().diagnostics(), &[diagnostic]);
        assert_eq!(computations.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn fact_cells_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<FactCell<u32>>();
        assert_send_sync::<Arc<FactCell<u32>>>();
    }

    fn join(
        handle: std::thread::ScopedJoinHandle<'_, Result<u32, FactQueryError>>,
    ) -> Result<u32, FactQueryError> {
        match handle.join() {
            Ok(result) => result,
            Err(_) => panic!("fact request thread should not panic"),
        }
    }
}
