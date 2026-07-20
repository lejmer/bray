use std::sync::{Condvar, Mutex, OnceLock};
use std::thread::{self, ThreadId};
use std::time::Duration;

#[cfg(test)]
use std::{fmt, sync::Arc};

use super::{CancellationToken, CompilationFactKey, EvaluationCommit, FactQueryError, FactRuntime};

const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub(crate) struct FactCell<T> {
    value: OnceLock<T>,
    state: Mutex<FactCellState>,
    changed: Condvar,
    #[cfg(test)]
    observer: Mutex<Option<FactCellTestObserver>>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FactCellTestEvent {
    Computing,
    Waiting,
    Computed,
}

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct FactCellTestObserver(Arc<dyn Fn(FactCellTestEvent) + Send + Sync>);

#[cfg(test)]
impl FactCellTestObserver {
    pub(crate) fn new(observe: impl Fn(FactCellTestEvent) + Send + Sync + 'static) -> Self {
        Self(Arc::new(observe))
    }

    fn observe(&self, event: FactCellTestEvent) {
        self.0(event);
    }
}

#[cfg(test)]
impl fmt::Debug for FactCellTestObserver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("FactCellTestObserver")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
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
            #[cfg(test)]
            observer: Mutex::new(None),
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

        runtime.request(&key)?;

        loop {
            cancellation.check()?;

            let mut state = self
                .state
                .lock()
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            match &*state {
                FactCellState::Ready(ready_key) => {
                    if ready_key != &key {
                        return Err(FactQueryError::InfrastructureFailure);
                    }

                    return self.ready_value();
                }
                FactCellState::Vacant => {
                    let thread = thread::current().id();
                    *state = FactCellState::Computing {
                        owner: thread,
                        key: key.clone(),
                    };

                    drop(state);

                    let mut publication = PublicationGuard::new(self, thread, key.clone());

                    #[cfg(test)]
                    self.observe(FactCellTestEvent::Computing)?;

                    let evaluation = runtime.begin(key.clone())?;

                    cancellation.check()?;

                    let compute = compute
                        .take()
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    let value = evaluation.run(compute)?;

                    #[cfg(test)]
                    self.observe(FactCellTestEvent::Computed)?;

                    cancellation.check()?;

                    let commit = evaluation.prepare()?;

                    self.publish(value, thread, key, commit)?;

                    publication.disarm();

                    return self.ready_value();
                }
                FactCellState::Computing {
                    owner,
                    key: computing_key,
                } => {
                    if computing_key != &key {
                        return Err(FactQueryError::InfrastructureFailure);
                    }

                    let owner = *owner;
                    let thread = thread::current().id();

                    if owner == thread {
                        return Err(FactQueryError::Cycle(
                            runtime.same_thread_cycle(key.clone())?,
                        ));
                    }

                    drop(state);

                    let waiting = runtime.wait_for(key.clone(), owner)?;

                    #[cfg(test)]
                    self.observe(FactCellTestEvent::Waiting)?;

                    let state = self
                        .state
                        .lock()
                        .map_err(|_| FactQueryError::InfrastructureFailure)?;

                    let same_evaluation = matches!(
                        &*state,
                        FactCellState::Computing {
                            owner: current_owner,
                            key: current_key,
                        } if *current_owner == owner && current_key == &key
                    );

                    if same_evaluation {
                        let waited = self
                            .changed
                            .wait_timeout(state, CANCELLATION_POLL_INTERVAL)
                            .map_err(|_| FactQueryError::InfrastructureFailure)?;

                        drop(waited);
                    } else {
                        drop(state);
                    }

                    drop(waiting);
                }
            }
        }
    }

    fn publish(
        &self,
        value: T,
        thread: ThreadId,
        key: CompilationFactKey,
        commit: EvaluationCommit<'_>,
    ) -> Result<(), FactQueryError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        if !matches!(
            &*state,
            FactCellState::Computing {
                owner,
                key: active_key,
            } if *owner == thread && active_key == &key
        ) {
            return Err(FactQueryError::InfrastructureFailure);
        }

        self.value
            .set(value)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        commit.commit();

        *state = FactCellState::Ready(key);

        self.changed.notify_all();

        Ok(())
    }

    fn ready_value(&self) -> Result<&T, FactQueryError> {
        self.value
            .get()
            .ok_or(FactQueryError::InfrastructureFailure)
    }

    #[cfg(test)]
    pub(crate) fn set_test_observer(
        &self,
        observer: FactCellTestObserver,
    ) -> Result<(), FactQueryError> {
        let mut current = self
            .observer
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        *current = Some(observer);

        Ok(())
    }

    #[cfg(test)]
    fn observe(&self, event: FactCellTestEvent) -> Result<(), FactQueryError> {
        let observer = self
            .observer
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?
            .clone();

        if let Some(observer) = observer {
            observer.observe(event);
        }

        Ok(())
    }

    fn abandon(&self, thread: ThreadId, key: CompilationFactKey) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        if !matches!(
            &*state,
            FactCellState::Computing {
                owner,
                key: active_key,
            } if *owner == thread && active_key == &key
        ) {
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
            self.cell.abandon(self.thread, self.key.clone());
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

    use super::{FactCell, FactCellState, FactCellTestEvent, FactCellTestObserver};
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

        let result = cell.get_or_compute(&runtime, key.clone(), &cancellation, || {
            cell.get_or_compute(&runtime, key.clone(), &cancellation, || Ok(2_u32))
                .copied()
        });

        let error = match result {
            Ok(_) => panic!("recursive fact request should report a cycle"),
            Err(error) => error,
        };

        let FactQueryError::Cycle(cycle) = error else {
            panic!("recursive fact request should report a cycle");
        };

        assert_eq!(cycle.facts(), &[key.clone(), key]);
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
    fn successful_facts_record_sorted_unique_direct_dependencies() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();

        let parent = FactCell::new();
        let syntax = FactCell::new();
        let declarations = FactCell::new();

        let parent_key = CompilationFactKey::CheckDiagnostics;
        let syntax_key = CompilationFactKey::SyntaxTree;
        let declaration_key = CompilationFactKey::DeclarationTable;

        let primed =
            syntax.get_or_compute(&runtime, syntax_key.clone(), &cancellation, || Ok(1_u32));

        assert_eq!(primed, Ok(&1));

        let result = parent.get_or_compute(&runtime, parent_key.clone(), &cancellation, || {
            syntax.get_or_compute(&runtime, syntax_key.clone(), &cancellation, || Ok(2_u32))?;
            declarations.get_or_compute(
                &runtime,
                declaration_key.clone(),
                &cancellation,
                || Ok(3_u32),
            )?;
            syntax.get_or_compute(&runtime, syntax_key.clone(), &cancellation, || Ok(4_u32))?;

            Ok(5_u32)
        });

        assert_eq!(result, Ok(&5));
        assert_eq!(
            runtime.dependencies(&parent_key),
            Ok(Some(vec![declaration_key, syntax_key].into_boxed_slice()))
        );
    }

    #[test]
    fn propagated_task_context_records_worker_dependencies() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();

        let parent = FactCell::new();
        let child = FactCell::new();

        let parent_key = CompilationFactKey::CheckDiagnostics;
        let child_key = CompilationFactKey::DeclarationTable;

        let result = parent.get_or_compute(&runtime, parent_key.clone(), &cancellation, || {
            let context = runtime.current_task_context()?;

            let child_value = std::thread::scope(|scope| {
                join(scope.spawn(|| {
                    context.run(|| {
                        child
                            .get_or_compute(&runtime, child_key.clone(), &cancellation, || {
                                Ok(2_u32)
                            })
                            .copied()
                    })
                }))
            })?;

            Ok(child_value + 1)
        });

        assert_eq!(result, Ok(&3));
        assert_eq!(
            runtime.dependencies(&parent_key),
            Ok(Some(vec![child_key].into_boxed_slice()))
        );
    }

    #[test]
    fn propagated_task_context_reports_worker_cycles_without_waiting() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();

        let parent = FactCell::new();
        let parent_key = CompilationFactKey::CheckDiagnostics;

        let result = parent.get_or_compute(&runtime, parent_key.clone(), &cancellation, || {
            let context = runtime.current_task_context()?;

            std::thread::scope(|scope| {
                join(scope.spawn(|| {
                    context.run(|| {
                        parent
                            .get_or_compute(&runtime, parent_key.clone(), &cancellation, || {
                                Ok(2_u32)
                            })
                            .copied()
                    })
                }))
            })
        });

        let Err(FactQueryError::Cycle(cycle)) = result else {
            panic!("propagated task cycle should be reported");
        };

        assert_eq!(cycle.facts(), &[parent_key.clone(), parent_key]);
    }

    #[test]
    fn abandoned_fact_dependencies_are_discarded_before_retry() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();

        let parent = FactCell::new();
        let abandoned_dependency = FactCell::new();
        let committed_dependency = FactCell::new();

        let parent_key = CompilationFactKey::CheckDiagnostics;
        let abandoned_key = CompilationFactKey::SyntaxTree;
        let committed_key = CompilationFactKey::DeclarationTable;

        let abandoned = parent.get_or_compute(&runtime, parent_key.clone(), &cancellation, || {
            abandoned_dependency
                .get_or_compute(&runtime, abandoned_key, &cancellation, || Ok(1_u32))?;

            Err(FactQueryError::InfrastructureFailure)
        });

        assert_eq!(abandoned, Err(FactQueryError::InfrastructureFailure));

        let retried = parent.get_or_compute(&runtime, parent_key.clone(), &cancellation, || {
            committed_dependency.get_or_compute(
                &runtime,
                committed_key.clone(),
                &cancellation,
                || Ok(2_u32),
            )?;

            Ok(3_u32)
        });

        assert_eq!(retried, Ok(&3));
        assert_eq!(
            runtime.dependencies(&parent_key),
            Ok(Some(vec![committed_key].into_boxed_slice()))
        );
    }

    #[test]
    fn failed_cache_publication_discards_dependencies() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();

        let parent = Arc::new(FactCell::new());
        let child = FactCell::new();

        let parent_key = CompilationFactKey::CheckDiagnostics;
        let child_key = CompilationFactKey::DeclarationTable;

        let weak_parent = Arc::downgrade(&parent);
        let observer = FactCellTestObserver::new(move |event| {
            if event != FactCellTestEvent::Computed {
                return;
            }

            let Some(parent) = weak_parent.upgrade() else {
                return;
            };

            let mut state = match parent.state.lock() {
                Ok(state) => state,
                Err(_) => panic!("test fact state should remain available"),
            };

            *state = FactCellState::Vacant;
        });

        assert_eq!(parent.set_test_observer(observer), Ok(()));

        let result = parent.get_or_compute(&runtime, parent_key.clone(), &cancellation, || {
            child.get_or_compute(&runtime, child_key, &cancellation, || Ok(1_u32))?;

            Ok(2_u32)
        });

        assert_eq!(result, Err(FactQueryError::InfrastructureFailure));
        assert_eq!(runtime.dependencies(&parent_key), Ok(None));
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
                    .get_or_compute(&runtime, first_key.clone(), &cancellation, || {
                        barrier.wait();

                        second
                            .get_or_compute(&runtime, second_key.clone(), &cancellation, || {
                                Ok(20_u32)
                            })
                            .copied()
                    })
                    .copied()
            });

            let second_handle = scope.spawn(|| {
                second
                    .get_or_compute(&runtime, second_key.clone(), &cancellation, || {
                        barrier.wait();

                        first
                            .get_or_compute(&runtime, first_key.clone(), &cancellation, || {
                                Ok(10_u32)
                            })
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

        let result = cell.get_or_compute(&runtime, key.clone(), &cancellation, || {
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

            let owner_key = key.clone();

            let owner = scope.spawn(move || {
                owner_cell
                    .get_or_compute(owner_runtime, owner_key, owner_token, || {
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

            let waiter_key = key;

            let waiter = scope.spawn(move || {
                let result = waiter_cell
                    .get_or_compute(waiter_runtime, waiter_key, waiter_token, || Ok(9_u32))
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
