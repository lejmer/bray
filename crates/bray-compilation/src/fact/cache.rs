use std::hash::Hash;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

#[cfg(test)]
use std::fmt;

use super::{
    CancellationToken, CompilationFactKey, EvaluationCommit, FactQueryError, FactRuntime,
    FactTaskIdentity, QueryPriority, QueryPriorityDemand, SharedCancellation,
};
use crate::profile::CompilationProfileOutcome;

const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub(crate) struct FactCell<T> {
    storage: Arc<FactCellStorage<T>>,
}

#[derive(Debug)]
struct FactCellStorage<T> {
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

#[derive(Clone, Debug)]
enum FactCellState {
    Vacant,
    Computing {
        task: FactTaskIdentity,
        key: CompilationFactKey,
        cancellation: SharedCancellation,
        priority: QueryPriorityDemand,
    },
    Ready(CompilationFactKey),
}

impl<T> FactCell<T> {
    pub(crate) fn new() -> Self {
        Self {
            storage: Arc::new(FactCellStorage {
                value: OnceLock::new(),
                state: Mutex::new(FactCellState::Vacant),
                changed: Condvar::new(),
                #[cfg(test)]
                observer: Mutex::new(None),
            }),
        }
    }

    pub(crate) fn get(&self) -> Option<&T> {
        self.storage.value.get()
    }

    #[cfg(test)]
    pub(crate) fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.storage, &other.storage)
    }

    #[cfg(test)]
    pub(crate) fn storage_reference_count(&self) -> usize {
        Arc::strong_count(&self.storage)
    }

    pub(crate) fn updated(
        &self,
        key: &CompilationFactKey,
        reusable: &std::collections::BTreeSet<CompilationFactKey>,
    ) -> Self {
        if !reusable.contains(key) {
            return Self::new();
        }

        self.reused(key).unwrap_or_default()
    }

    pub(super) fn reused(&self, key: &CompilationFactKey) -> Option<Self> {
        // Reused cells share one immutable publication allocation across snapshots.
        self.is_ready_for(key).then(|| self.clone())
    }

    pub(super) fn is_ready_for(&self, key: &CompilationFactKey) -> bool {
        let Ok(state) = self.storage.state.lock() else {
            return false;
        };

        matches!(&*state, FactCellState::Ready(ready_key) if ready_key == key)
    }

    pub(super) fn is_vacant(&self) -> bool {
        let Ok(state) = self.storage.state.lock() else {
            return false;
        };

        matches!(*state, FactCellState::Vacant)
    }

    pub(crate) fn get_or_compute(
        &self,
        runtime: &FactRuntime,
        key: CompilationFactKey,
        cancellation: &CancellationToken,
        compute: impl FnOnce() -> Result<T, FactQueryError> + Send,
    ) -> Result<&T, FactQueryError>
    where
        T: Hash + Send,
    {
        let priority = runtime.current_priority()?.unwrap_or(QueryPriority::Normal);

        self.get_or_compute_with_priority(runtime, key, cancellation, priority, compute)
    }

    pub(crate) fn get_or_compute_with_priority(
        &self,
        runtime: &FactRuntime,
        key: CompilationFactKey,
        cancellation: &CancellationToken,
        priority: QueryPriority,
        compute: impl FnOnce() -> Result<T, FactQueryError> + Send,
    ) -> Result<&T, FactQueryError>
    where
        T: Hash + Send,
    {
        self.get_or_compute_requested_with_cycle_key_and_priority(
            runtime,
            key.clone(),
            key,
            cancellation,
            priority,
            |_| compute(),
        )
    }

    pub(crate) fn get_or_compute_requested(
        &self,
        runtime: &FactRuntime,
        key: CompilationFactKey,
        cancellation: &CancellationToken,
        priority: QueryPriority,
        compute: impl FnOnce(&CancellationToken) -> Result<T, FactQueryError> + Send,
    ) -> Result<&T, FactQueryError>
    where
        T: Hash + Send,
    {
        self.get_or_compute_requested_with_cycle_key_and_priority(
            runtime,
            key.clone(),
            key,
            cancellation,
            priority,
            compute,
        )
    }

    pub(crate) fn get_or_compute_with_cycle_key(
        &self,
        runtime: &FactRuntime,
        key: CompilationFactKey,
        cycle_key: CompilationFactKey,
        cancellation: &CancellationToken,
        compute: impl FnOnce() -> Result<T, FactQueryError> + Send,
    ) -> Result<&T, FactQueryError>
    where
        T: Hash + Send,
    {
        let priority = runtime.current_priority()?.unwrap_or(QueryPriority::Normal);

        self.get_or_compute_requested_with_cycle_key_and_priority(
            runtime,
            key,
            cycle_key,
            cancellation,
            priority,
            |_| compute(),
        )
    }

    fn get_or_compute_requested_with_cycle_key_and_priority(
        &self,
        runtime: &FactRuntime,
        key: CompilationFactKey,
        cycle_key: CompilationFactKey,
        cancellation: &CancellationToken,
        priority: QueryPriority,
        compute: impl FnOnce(&CancellationToken) -> Result<T, FactQueryError> + Send,
    ) -> Result<&T, FactQueryError>
    where
        T: Hash + Send,
    {
        let mut compute = Some(compute);

        let profile = runtime
            .profile()
            .map(|profile| (profile, crate::profile::ProfileQueryKind::from_key(&key)));

        let mut cache_outcome_recorded = false;

        if let Some((profile, query)) = profile {
            profile.record_query_request(query);
        }

        runtime.request_with_cycle_key(&key, &cycle_key)?;

        loop {
            cancellation.check()?;

            let mut state = self
                .storage
                .state
                .lock()
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            match &*state {
                FactCellState::Ready(ready_key) => {
                    if ready_key != &key {
                        return Err(FactQueryError::InfrastructureFailure);
                    }

                    record_cache_outcome(profile, &mut cache_outcome_recorded, true);

                    return self.ready_value();
                }
                FactCellState::Vacant => {
                    record_cache_outcome(profile, &mut cache_outcome_recorded, false);

                    // The task, cell state, and rollback guard independently retain this key.
                    let context = runtime.task_with_cycle_key(key.clone(), cycle_key.clone())?;
                    let task = context.identity();
                    let shared_cancellation = SharedCancellation::new();
                    let _interest = shared_cancellation.register(cancellation)?;
                    let shared_priority = QueryPriorityDemand::new(priority);

                    *state = FactCellState::Computing {
                        task,
                        key: key.clone(),
                        cancellation: shared_cancellation.clone(),
                        priority: shared_priority.clone(),
                    };

                    drop(state);

                    let mut publication = PublicationGuard::new(self, task, key.clone());
                    let evaluation = runtime.begin(context)?;

                    #[cfg(test)]
                    self.observe(FactCellTestEvent::Computing)?;

                    let compute = compute
                        .take()
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    let value = runtime.run_demand(&shared_priority, || {
                        let span = profile.map(|(profile, _)| {
                            profile.start_query(
                                crate::profile::ProfileOperation::QueryEvaluation,
                                &key,
                            )
                        });

                        let value = evaluation.run(|| compute(shared_cancellation.token()));

                        if let Some(span) = span {
                            span.finish(profile_outcome(&value));
                        }

                        value
                    })?;

                    #[cfg(test)]
                    self.observe(FactCellTestEvent::Computed)?;

                    shared_cancellation.token().check()?;

                    let commit = evaluation.prepare(&value)?;

                    self.publish(value, task, key, commit)?;

                    publication.disarm();

                    cancellation.check()?;

                    return self.ready_value();
                }
                FactCellState::Computing {
                    task,
                    key: computing_key,
                    cancellation: shared_cancellation,
                    priority: shared_priority,
                } => {
                    if computing_key != &key {
                        return Err(FactQueryError::InfrastructureFailure);
                    }

                    let task = *task;
                    let shared_cancellation = shared_cancellation.clone();
                    let shared_priority = shared_priority.clone();

                    let current_task = runtime
                        .current_task_context()
                        .ok()
                        .map(|context| context.identity());

                    if current_task == Some(task) {
                        return Err(FactQueryError::Cycle(runtime.same_task_cycle(&key)?));
                    }

                    record_cache_outcome(profile, &mut cache_outcome_recorded, false);

                    drop(state);

                    let _interest = shared_cancellation.register(cancellation)?;

                    runtime.promote_priority(&shared_priority, priority);

                    let Some(waiting) = runtime.wait_for(&key, task)? else {
                        continue;
                    };

                    #[cfg(test)]
                    self.observe(FactCellTestEvent::Waiting)?;

                    let wait_span = profile.map(|(profile, _)| {
                        profile.start_query(crate::profile::ProfileOperation::DependencyWait, &key)
                    });

                    let state = self
                        .storage
                        .state
                        .lock()
                        .map_err(|_| FactQueryError::InfrastructureFailure)?;

                    let same_evaluation = matches!(
                        &*state,
                        FactCellState::Computing {
                            task: current_task,
                            key: current_key,
                            ..
                        } if *current_task == task && current_key == &key
                    );

                    if same_evaluation {
                        let waited = self
                            .storage
                            .changed
                            .wait_timeout(state, CANCELLATION_POLL_INTERVAL)
                            .map_err(|_| FactQueryError::InfrastructureFailure)?;

                        drop(waited);
                    } else {
                        drop(state);
                    }

                    if let Some(span) = wait_span {
                        span.finish(CompilationProfileOutcome::Completed);
                    }

                    drop(waiting);
                }
            }
        }
    }

    fn publish(
        &self,
        value: T,
        task: FactTaskIdentity,
        key: CompilationFactKey,
        commit: EvaluationCommit<'_>,
    ) -> Result<(), FactQueryError> {
        let mut state = self
            .storage
            .state
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        if !matches!(
            &*state,
            FactCellState::Computing {
                task: active_task,
                key: active_key,
                ..
            } if *active_task == task && active_key == &key
        ) {
            return Err(FactQueryError::InfrastructureFailure);
        }

        self.storage
            .value
            .set(value)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        commit.commit();

        *state = FactCellState::Ready(key);

        self.storage.changed.notify_all();

        Ok(())
    }

    fn ready_value(&self) -> Result<&T, FactQueryError> {
        self.storage
            .value
            .get()
            .ok_or(FactQueryError::InfrastructureFailure)
    }

    #[cfg(test)]
    pub(crate) fn set_test_observer(
        &self,
        observer: FactCellTestObserver,
    ) -> Result<(), FactQueryError> {
        let mut current = self
            .storage
            .observer
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        *current = Some(observer);

        Ok(())
    }

    #[cfg(test)]
    fn observe(&self, event: FactCellTestEvent) -> Result<(), FactQueryError> {
        let observer = self
            .storage
            .observer
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?
            .clone();

        if let Some(observer) = observer {
            observer.observe(event);
        }

        Ok(())
    }

    fn abandon(&self, task: FactTaskIdentity, key: &CompilationFactKey) {
        let Ok(mut state) = self.storage.state.lock() else {
            return;
        };

        if !matches!(
            &*state,
            FactCellState::Computing {
                task: active_task,
                key: active_key,
                ..
            } if *active_task == task && active_key == key
        ) {
            return;
        }

        *state = if self.storage.value.get().is_some() {
            // Recovery must retain the initialized value's cache identity after the guard drops.
            FactCellState::Ready(key.clone())
        } else {
            FactCellState::Vacant
        };

        self.storage.changed.notify_all();
    }
}

fn record_cache_outcome(
    profile: Option<(
        &crate::profile::ProfileSession,
        crate::profile::ProfileQueryKind,
    )>,
    recorded: &mut bool,
    is_hit: bool,
) {
    if *recorded {
        return;
    }

    if let Some((profile, query)) = profile {
        if is_hit {
            profile.record_query_cache_hit(query);
        } else {
            profile.record_query_cache_miss(query);
        }
    }

    *recorded = true;
}

fn profile_outcome<T>(result: &Result<T, FactQueryError>) -> CompilationProfileOutcome {
    match result {
        Ok(_) => CompilationProfileOutcome::Completed,
        Err(FactQueryError::Cancelled) => CompilationProfileOutcome::Cancelled,
        Err(_) => CompilationProfileOutcome::Failed,
    }
}

impl<T> Clone for FactCell<T> {
    fn clone(&self) -> Self {
        Self {
            storage: Arc::clone(&self.storage),
        }
    }
}

impl<T> Default for FactCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

struct PublicationGuard<'a, T> {
    cell: &'a FactCell<T>,
    task: FactTaskIdentity,
    key: CompilationFactKey,
    active: bool,
}

impl<'a, T> PublicationGuard<'a, T> {
    fn new(cell: &'a FactCell<T>, task: FactTaskIdentity, key: CompilationFactKey) -> Self {
        Self {
            cell,
            task,
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
            self.cell.abandon(self.task, &self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Barrier, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    use std::time::Duration;

    use bray_diagnostics::{
        Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticResult, SeverityKind,
    };

    use super::{FactCell, FactCellState, FactCellTestEvent, FactCellTestObserver};
    use crate::fact::task::FactTaskContext;
    use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, FactRuntime};
    use crate::test_support::FactTestGate;
    use crate::{
        CompilationProfileConfiguration, CompilationProfileContext, CompilationProfileMode,
        WorkerBudget,
    };

    #[test]
    fn concurrent_requests_compute_one_value() {
        let runtime = FactRuntime::with_profile(
            WorkerBudget::default(),
            Some((
                CompilationProfileConfiguration::new(CompilationProfileMode::Summary),
                CompilationProfileContext {
                    package: "test.package".to_owned(),
                    product: "test.product".to_owned(),
                    target: "test-target".to_owned(),
                },
            )),
        );

        let cancellation = CancellationToken::new();

        let cell = FactCell::new();
        let computations = AtomicUsize::new(0);
        let gate = FactTestGate::holding(FactCellTestEvent::Computing);

        assert_eq!(cell.set_test_observer(gate.observer()), Ok(()));

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

                                Ok(42_u32)
                            },
                        )
                        .copied()
                    })
                })
                .collect::<Vec<_>>();

            gate.wait_until_observed(FactCellTestEvent::Computing, 1);
            gate.wait_until_observed(FactCellTestEvent::Waiting, 7);
            gate.release();

            for handle in handles {
                let result = join(handle);

                assert_eq!(result, Ok(42));
            }
        });

        assert_eq!(computations.load(Ordering::SeqCst), 1);

        let report = runtime
            .profile_report()
            .unwrap_or_else(|| panic!("profiled fact runtime must report"));

        let query = report
            .queries
            .iter()
            .find(|query| {
                report
                    .query_descriptor(query.id)
                    .is_some_and(|descriptor| descriptor.name == "syntax_tree")
            })
            .unwrap_or_else(|| panic!("syntax-tree query statistics must be present"));

        assert_eq!(query.requests, 8);
        assert_eq!(query.cache_hits, 0);
        assert_eq!(query.cache_misses, 8);
        assert_eq!(query.evaluations, 1);
    }

    #[test]
    fn request_priority_does_not_change_fact_identity_or_value() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cell = FactCell::new();
        let computations = AtomicUsize::new(0);

        let first = cell.get_or_compute_with_priority(
            &runtime,
            CompilationFactKey::SyntaxTree,
            &cancellation,
            crate::QueryPriority::Background,
            || {
                computations.fetch_add(1, Ordering::SeqCst);

                Ok(7_u32)
            },
        );

        let second = cell.get_or_compute_with_priority(
            &runtime,
            CompilationFactKey::SyntaxTree,
            &cancellation,
            crate::QueryPriority::Interactive,
            || {
                computations.fetch_add(1, Ordering::SeqCst);

                Ok(9_u32)
            },
        );

        assert_eq!(first, Ok(&7));
        assert_eq!(second, Ok(&7));
        assert_eq!(computations.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn independent_queries_compute_concurrently() {
        let runtime = runtime(2);
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
    fn distinct_cache_keys_with_one_cycle_identity_compute_concurrently() {
        let runtime = runtime(2);
        let cancellation = CancellationToken::new();

        let first = FactCell::new();
        let second = FactCell::new();
        let barrier = Barrier::new(2);
        let cycle_key = CompilationFactKey::CheckDiagnostics;

        let results = std::thread::scope(|scope| {
            let first_handle = scope.spawn(|| {
                first
                    .get_or_compute_with_cycle_key(
                        &runtime,
                        CompilationFactKey::SyntaxTree,
                        cycle_key.clone(),
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
                    .get_or_compute_with_cycle_key(
                        &runtime,
                        CompilationFactKey::DeclarationTable,
                        cycle_key.clone(),
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
    fn same_task_cycles_are_reported_before_waiting() {
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
    fn distinct_cache_keys_report_their_shared_cycle_identity() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let first = FactCell::new();
        let second = FactCell::new();

        let cycle_key = CompilationFactKey::CheckDiagnostics;

        let result = first.get_or_compute_with_cycle_key(
            &runtime,
            CompilationFactKey::SyntaxTree,
            cycle_key.clone(),
            &cancellation,
            || {
                second
                    .get_or_compute_with_cycle_key(
                        &runtime,
                        CompilationFactKey::DeclarationTable,
                        cycle_key.clone(),
                        &cancellation,
                        || Ok(2_u32),
                    )
                    .copied()
            },
        );

        let Err(FactQueryError::Cycle(cycle)) = result else {
            panic!("shared semantic identity should report a cycle");
        };

        assert_eq!(cycle.facts(), &[cycle_key.clone(), cycle_key]);
        assert!(first.get().is_none());
        assert!(second.get().is_none());
    }

    #[test]
    fn cache_cells_reject_reuse_for_another_semantic_key() {
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
        let runtime = runtime(2);
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
        let runtime = runtime(2);
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
    fn propagated_task_context_reports_indirect_worker_wait_cycles() {
        let runtime = runtime(3);
        let cancellation = CancellationToken::new();

        let parent = FactCell::new();
        let child = FactCell::new();

        let parent_key = CompilationFactKey::CheckDiagnostics;
        let child_key = CompilationFactKey::DeclarationTable;

        let start_child_dependency = Barrier::new(2);

        let (parent_waiting_sender, parent_waiting_receiver) = mpsc::sync_channel(1);

        let parent_waiting_receiver = Mutex::new(parent_waiting_receiver);

        let observer = FactCellTestObserver::new(move |event| {
            if event == FactCellTestEvent::Waiting {
                let _ = parent_waiting_sender.try_send(());
            }
        });

        assert_eq!(parent.set_test_observer(observer), Ok(()));

        let (parent_result, child_result) = std::thread::scope(|scope| {
            let child_handle = scope.spawn(|| {
                child
                    .get_or_compute(&runtime, child_key.clone(), &cancellation, || {
                        start_child_dependency.wait();

                        parent.get_or_compute(
                            &runtime,
                            parent_key.clone(),
                            &cancellation,
                            || Ok(3_u32),
                        )?;

                        Ok(4_u32)
                    })
                    .copied()
            });

            let parent_result =
                parent.get_or_compute(&runtime, parent_key.clone(), &cancellation, || {
                    let context = runtime.current_task_context()?;

                    start_child_dependency.wait();

                    let parent_waiting = parent_waiting_receiver
                        .lock()
                        .map_err(|_| FactQueryError::InfrastructureFailure)?
                        .recv_timeout(Duration::from_secs(1));

                    if parent_waiting.is_err() {
                        cancellation.cancel();

                        return Err(FactQueryError::InfrastructureFailure);
                    }

                    let (result_sender, result_receiver) = mpsc::sync_channel(1);

                    let worker_result = std::thread::scope(|worker_scope| {
                        worker_scope.spawn(|| {
                            let result = context.run(|| {
                                child
                                    .get_or_compute(
                                        &runtime,
                                        child_key.clone(),
                                        &cancellation,
                                        || Ok(5_u32),
                                    )
                                    .copied()
                            });

                            let _ = result_sender.send(result);
                        });

                        match result_receiver.recv_timeout(Duration::from_secs(1)) {
                            Ok(result) => result,
                            Err(_) => {
                                cancellation.cancel();

                                Err(FactQueryError::InfrastructureFailure)
                            }
                        }
                    })?;

                    Ok(worker_result + 1)
                });

            let child_result = join(child_handle);

            (parent_result, child_result)
        });

        let Err(FactQueryError::Cycle(cycle)) = parent_result else {
            panic!("indirect propagated task cycle should be reported");
        };

        assert_eq!(cycle.facts(), &[parent_key.clone(), child_key, parent_key]);
        assert_eq!(child_result, Ok(4));
    }

    #[test]
    fn duplicate_propagated_waits_preserve_the_edge_until_the_last_guard_drops() {
        let runtime = FactRuntime::default();

        let parent_key = CompilationFactKey::CheckDiagnostics;
        let child_key = CompilationFactKey::DeclarationTable;

        let parent_context = task_context(&runtime, parent_key.clone());
        let parent_identity = parent_context.identity();

        let parent_evaluation = match runtime.begin(parent_context.clone()) {
            Ok(evaluation) => evaluation,
            Err(error) => panic!("parent evaluation should begin: {error:?}"),
        };

        let child_context = task_context(&runtime, child_key.clone());
        let child_identity = child_context.identity();

        let child_evaluation = match runtime.begin(child_context.clone()) {
            Ok(evaluation) => evaluation,
            Err(error) => panic!("child evaluation should begin: {error:?}"),
        };

        let (registered_sender, registered_receiver) = mpsc::sync_channel(1);

        let (release_sender, release_receiver) = mpsc::sync_channel(1);

        let cycle = std::thread::scope(|scope| {
            let first_runtime = &runtime;
            let first_context = parent_context.clone();
            let first_child_key = child_key.clone();

            let first = scope.spawn(move || {
                first_context.run(|| {
                    let Some(waiting) = first_runtime.wait_for(&first_child_key, child_identity)?
                    else {
                        return Err(FactQueryError::InfrastructureFailure);
                    };

                    if registered_sender.send(()).is_err() {
                        return Err(FactQueryError::InfrastructureFailure);
                    }

                    if release_receiver.recv().is_err() {
                        return Err(FactQueryError::InfrastructureFailure);
                    }

                    drop(waiting);

                    Ok(())
                })
            });

            if registered_receiver.recv().is_err() {
                panic!("first propagated wait should be registered");
            }

            let second_runtime = &runtime;
            let second_context = parent_context.clone();
            let second_child_key = child_key.clone();

            let second = scope.spawn(move || {
                second_context.run(|| {
                    let Some(waiting) =
                        second_runtime.wait_for(&second_child_key, child_identity)?
                    else {
                        return Err(FactQueryError::InfrastructureFailure);
                    };

                    drop(waiting);

                    Ok(())
                })
            });

            assert_eq!(join(second), Ok(()));

            let cycle =
                child_context.run(|| match runtime.wait_for(&parent_key, parent_identity) {
                    Err(FactQueryError::Cycle(cycle)) => Ok(cycle),
                    Ok(_) | Err(_) => Err(FactQueryError::InfrastructureFailure),
                });

            if release_sender.send(()).is_err() {
                panic!("first propagated wait should be released");
            }

            assert_eq!(join(first), Ok(()));

            cycle
        });

        let cycle = match cycle {
            Ok(cycle) => cycle,
            Err(error) => panic!("reverse wait should report the preserved edge: {error:?}"),
        };

        assert_eq!(
            cycle.facts(),
            &[parent_key, child_key, CompilationFactKey::CheckDiagnostics]
        );

        drop(parent_evaluation);
        drop(child_evaluation);
    }

    #[test]
    fn stale_owner_observations_retry_after_evaluation_handoff() {
        let runtime = FactRuntime::default();
        let key = CompilationFactKey::DeclarationTable;

        let first_context = task_context(&runtime, key.clone());
        let observed_owner = first_context.identity();

        let first = match runtime.begin(first_context) {
            Ok(evaluation) => evaluation,
            Err(error) => panic!("first evaluation should begin: {error:?}"),
        };

        drop(first);

        let second_context = task_context(&runtime, key.clone());

        let second = match runtime.begin(second_context) {
            Ok(evaluation) => evaluation,
            Err(error) => panic!("second evaluation should begin: {error:?}"),
        };

        assert!(
            runtime
                .wait_for(&key, observed_owner)
                .is_ok_and(|waiting| waiting.is_none())
        );

        drop(second);
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

            let mut state = match parent.storage.state.lock() {
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
        let runtime = runtime(2);
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
        let runtime = runtime(2);

        let owner_cancellation = CancellationToken::new();
        let waiter_cancellation = CancellationToken::new();

        let cell = FactCell::new();
        let key = CompilationFactKey::SyntaxTree;

        let (started_sender, started_receiver) = mpsc::channel();

        let (release_sender, release_receiver) = mpsc::channel();

        let (result_sender, result_receiver) = mpsc::channel();

        let (waiting_sender, waiting_receiver) = mpsc::sync_channel(1);

        let observer = FactCellTestObserver::new(move |event| {
            if event == FactCellTestEvent::Waiting {
                let _ = waiting_sender.try_send(());
            }
        });

        assert_eq!(cell.set_test_observer(observer), Ok(()));

        let observed = std::thread::scope(|scope| {
            let owner_cell = &cell;
            let owner_runtime = &runtime;
            let owner_token = &owner_cancellation;

            let owner_key = key.clone();

            let owner = scope.spawn(move || {
                owner_cell
                    .get_or_compute(owner_runtime, owner_key, owner_token, move || {
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

            waiting_receiver
                .recv_timeout(Duration::from_secs(1))
                .unwrap_or_else(|_| panic!("waiting fact request must be observed"));

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
    fn cancelling_the_owner_keeps_shared_work_needed_by_a_waiter() {
        let runtime = runtime(2);
        let owner_cancellation = CancellationToken::new();
        let waiter_cancellation = CancellationToken::new();
        let cell = FactCell::new();
        let key = CompilationFactKey::SyntaxTree;

        let (started_sender, started_receiver) = mpsc::channel();

        let (waiting_sender, waiting_receiver) = mpsc::sync_channel(1);

        let (release_sender, release_receiver) = mpsc::channel();

        let release_receiver = Mutex::new(release_receiver);

        let observer = FactCellTestObserver::new(move |event| {
            if event == FactCellTestEvent::Waiting {
                let _ = waiting_sender.try_send(());
            }
        });

        assert_eq!(cell.set_test_observer(observer), Ok(()));

        let (owner_result, waiter_result) = std::thread::scope(|scope| {
            let owner_cell = &cell;
            let owner_runtime = &runtime;
            let owner_key = key.clone();
            let owner_token = &owner_cancellation;
            let owner_release = &release_receiver;

            let owner = scope.spawn(move || {
                owner_cell
                    .get_or_compute_requested(
                        owner_runtime,
                        owner_key,
                        owner_token,
                        crate::QueryPriority::Background,
                        |shared_cancellation| {
                            started_sender
                                .send(())
                                .unwrap_or_else(|_| panic!("test must observe shared work"));

                            owner_release
                                .lock()
                                .unwrap_or_else(|_| panic!("release channel must remain available"))
                                .recv()
                                .unwrap_or_else(|_| panic!("test must release shared work"));

                            shared_cancellation.check()?;

                            Ok(5_u32)
                        },
                    )
                    .copied()
            });

            started_receiver
                .recv()
                .unwrap_or_else(|_| panic!("shared work must start"));

            let waiter = scope.spawn(|| {
                cell.get_or_compute_with_priority(
                    &runtime,
                    key,
                    &waiter_cancellation,
                    crate::QueryPriority::Interactive,
                    || Ok(9_u32),
                )
                .copied()
            });

            waiting_receiver
                .recv_timeout(Duration::from_secs(1))
                .unwrap_or_else(|_| panic!("interactive request must wait for shared work"));

            owner_cancellation.cancel();

            release_sender
                .send(())
                .unwrap_or_else(|_| panic!("test must release shared work"));

            (join(owner), join(waiter))
        });

        assert_eq!(owner_result, Err(FactQueryError::Cancelled));
        assert_eq!(waiter_result, Ok(5));
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

    fn task_context(runtime: &FactRuntime, key: CompilationFactKey) -> FactTaskContext {
        match runtime.task_with_cycle_key(key.clone(), key) {
            Ok(context) => context,
            Err(error) => panic!("fact task should be allocated: {error:?}"),
        }
    }

    fn runtime(workers: usize) -> FactRuntime {
        let worker_budget = crate::WorkerBudget::new(workers)
            .unwrap_or_else(|error| panic!("test worker budget must be valid: {error:?}"));

        FactRuntime::new(worker_budget)
    }

    fn join<T>(
        handle: std::thread::ScopedJoinHandle<'_, Result<T, FactQueryError>>,
    ) -> Result<T, FactQueryError> {
        match handle.join() {
            Ok(result) => result,
            Err(_) => panic!("fact request thread should not panic"),
        }
    }
}
