use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use super::{
    CompilationFactKey, CompilationInputKey, FactCycle, FactFingerprint, FactQueryError,
    FactRuntimeFailure, FactTaskPhase, LocalStateFailure, TaskContextIdentity, TaskLocalOperation,
    TaskOperation,
};

thread_local! {
    static LOCAL_EVALUATIONS: RefCell<Vec<FactTaskContext>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Debug)]
pub(crate) struct FactTaskContext {
    data: Arc<FactTaskData>,
}

#[derive(Debug)]
struct FactTaskData {
    runtime: RuntimeIdentity,
    identity: FactTaskIdentity,
    key: CompilationFactKey,
    cycle_key: CompilationFactKey,
    fixed_inputs: AtomicU32,
    frozen_facts: AtomicU8,
    state: Mutex<FactTaskState>,
}

#[derive(Debug)]
struct FactTaskState {
    phase: FactTaskPhase,
    dependencies: BTreeSet<CompilationFactKey>,
    inputs: BTreeMap<CompilationInputKey, FactFingerprint>,
}

pub(crate) struct RecordedDependencies {
    pub(crate) facts: BTreeSet<CompilationFactKey>,
    pub(crate) inputs: BTreeMap<CompilationInputKey, FactFingerprint>,
    pub(crate) fixed_inputs: u32,
    pub(crate) frozen_facts: u8,
}

impl FactTaskContext {
    pub(crate) fn with_cycle_key(
        runtime: RuntimeIdentity,
        identity: FactTaskIdentity,
        key: CompilationFactKey,
        cycle_key: CompilationFactKey,
    ) -> Self {
        Self {
            data: Arc::new(FactTaskData {
                runtime,
                identity,
                key,
                cycle_key,
                fixed_inputs: AtomicU32::new(0),
                frozen_facts: AtomicU8::new(0),
                state: Mutex::new(FactTaskState {
                    phase: FactTaskPhase::Recording,
                    dependencies: BTreeSet::new(),
                    inputs: BTreeMap::new(),
                }),
            }),
        }
    }

    pub(crate) fn key(&self) -> &CompilationFactKey {
        &self.data.key
    }

    pub(crate) fn identity(&self) -> FactTaskIdentity {
        self.data.identity
    }

    pub(crate) fn finish(&self) -> Result<RecordedDependencies, FactQueryError> {
        let mut state = self.state()?;

        if state.phase != FactTaskPhase::Recording {
            return Err(self.invalid_state(TaskOperation::Finish, state.phase));
        }

        state.phase = FactTaskPhase::Finished;

        Ok(RecordedDependencies {
            facts: std::mem::take(&mut state.dependencies),
            inputs: std::mem::take(&mut state.inputs),
            fixed_inputs: self.data.fixed_inputs.load(Ordering::Acquire),
            frozen_facts: self.data.frozen_facts.load(Ordering::Acquire),
        })
    }

    pub(crate) fn discard(&self) {
        // Cleanup cannot report failure from Drop paths. A poisoned task is already unavailable
        // to every fallible operation through `state`.
        let Ok(mut state) = self.data.state.lock() else {
            return;
        };

        state.phase = FactTaskPhase::Discarded;
        state.dependencies.clear();
        state.inputs.clear();
    }

    pub(crate) fn run<T>(
        &self,
        operation: impl FnOnce() -> Result<T, FactQueryError>,
    ) -> Result<T, FactQueryError> {
        local_evaluations(TaskLocalOperation::Enter, |active| {
            // Worker-local stacks share the task state while retaining independent stack storage.
            active.push(self.clone());

            Ok(())
        })?;

        let _guard = LocalTaskGuard { context: self };

        operation()
    }

    fn record(&self, key: &CompilationFactKey) -> Result<(), FactQueryError> {
        let mut state = self.state()?;

        if state.phase != FactTaskPhase::Recording {
            return Err(self.invalid_state(TaskOperation::RecordFact, state.phase));
        }

        // The dependency graph must own its keys after the accessor returns.
        state.dependencies.insert(key.clone());

        Ok(())
    }

    fn record_input(
        &self,
        key: &CompilationInputKey,
        fingerprint: FactFingerprint,
    ) -> Result<(), FactQueryError> {
        let mut state = self.state()?;

        if state.phase != FactTaskPhase::Recording {
            return Err(self.invalid_state(TaskOperation::RecordInput, state.phase));
        }

        if let Some(previous) = state.inputs.insert(key.clone(), fingerprint) {
            assert_eq!(
                previous,
                fingerprint,
                "input {key:?} changed during fact {:?}, task {:?}",
                self.key(),
                self.identity()
            );
        }

        Ok(())
    }

    fn record_fixed_input(&self, bit: u32) -> Result<(), FactQueryError> {
        let state = self.state()?;

        if state.phase != FactTaskPhase::Recording {
            return Err(self.invalid_state(TaskOperation::RecordFixedInput, state.phase));
        }

        self.data.fixed_inputs.fetch_or(bit, Ordering::AcqRel);

        Ok(())
    }

    fn record_frozen_fact(&self, bit: u8) -> Result<(), FactQueryError> {
        let state = self.state()?;

        if state.phase != FactTaskPhase::Recording {
            return Err(self.invalid_state(TaskOperation::RecordFrozenFact, state.phase));
        }

        self.data.frozen_facts.fetch_or(bit, Ordering::AcqRel);

        Ok(())
    }

    fn state(&self) -> Result<std::sync::MutexGuard<'_, FactTaskState>, FactQueryError> {
        self.data.state.lock().map_err(|_| {
            FactRuntimeFailure::SynchronizationPoisoned {
                component: super::SynchronizationComponent::TaskDependencies,
                fact: Some(self.key().clone()),
                task: Some(self.identity()),
            }
            .into()
        })
    }

    fn invalid_state(&self, operation: TaskOperation, actual: FactTaskPhase) -> FactQueryError {
        // The failure outlives the task-state lock and therefore owns the stable fact key.
        FactRuntimeFailure::InvalidTaskState {
            operation,
            expected: FactTaskPhase::Recording,
            actual,
            task: self.identity(),
            fact: self.key().clone(),
        }
        .into()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct RuntimeIdentity(pub(crate) usize);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct FactTaskIdentity(pub(crate) u64);

pub(crate) fn check_request_cycle(
    runtime: RuntimeIdentity,
    cycle_key: &CompilationFactKey,
) -> Result<(), FactQueryError> {
    local_evaluations(TaskLocalOperation::Cycle, |active| {
        let Some(context) = active.last() else {
            return Ok(());
        };

        if context.data.runtime != runtime {
            return Ok(());
        }

        if let Some(cycle) = task_cycle(active, runtime, cycle_key) {
            return Err(FactQueryError::Cycle(cycle));
        }

        Ok(())
    })
}

pub(crate) fn record_completed_request(
    runtime: RuntimeIdentity,
    key: &CompilationFactKey,
) -> Result<(), FactQueryError> {
    local_evaluations(TaskLocalOperation::Current, |active| {
        let Some(context) = active.last() else {
            return Ok(());
        };

        if context.data.runtime != runtime {
            return Ok(());
        }

        context.record(key)
    })
}

pub(crate) fn record_input(
    runtime: RuntimeIdentity,
    key: &CompilationInputKey,
    fingerprint: Option<FactFingerprint>,
) -> Result<(), FactQueryError> {
    local_evaluations(TaskLocalOperation::Current, |active| {
        let Some(context) = active.last() else {
            return Ok(());
        };

        if context.data.runtime != runtime {
            return Ok(());
        }

        if let Some(bit) = key.fixed_bit() {
            return context.record_fixed_input(bit);
        }

        let fingerprint = fingerprint.unwrap_or_else(|| {
            panic!(
                "input {key:?} has no fingerprint for fact {:?}, task {:?}",
                context.key(),
                context.identity()
            )
        });

        context.record_input(key, fingerprint)
    })
}

pub(crate) fn record_frozen_fact(runtime: RuntimeIdentity, bit: u8) -> Result<(), FactQueryError> {
    local_evaluations(TaskLocalOperation::Current, |active| {
        let Some(context) = active.last() else {
            return Ok(());
        };

        if context.data.runtime == runtime {
            context.record_frozen_fact(bit)?;
        }

        Ok(())
    })
}

pub(crate) fn current_context(
    runtime: RuntimeIdentity,
) -> Result<Option<FactTaskContext>, FactQueryError> {
    local_evaluations(TaskLocalOperation::Current, |active| {
        let Some(context) = active.last() else {
            return Ok(None);
        };

        if context.data.runtime != runtime {
            return Err(FactRuntimeFailure::InvalidTaskContext {
                expected_runtime: runtime,
                actual: TaskContextIdentity {
                    runtime: context.data.runtime,
                    task: context.identity(),
                    fact: context.key().clone(),
                },
            }
            .into());
        }

        // Scoped workers share dependency state without sharing their local context stacks.
        Ok(Some(context.clone()))
    })
}

pub(crate) fn capture_evaluations() -> Result<Vec<FactTaskContext>, FactQueryError> {
    // Scheduled jobs need independent stack storage while sharing each task's Arc-backed state.
    local_evaluations(TaskLocalOperation::Capture, |active| Ok(active.clone()))
}

pub(crate) fn run_with_evaluations<T>(
    evaluations: &[FactTaskContext],
    operation: impl FnOnce() -> Result<T, FactQueryError>,
) -> Result<T, FactQueryError> {
    // Each worker owns its local stack while retaining the caller's Arc-backed task contexts.
    let previous = local_evaluations(TaskLocalOperation::Replace, |active| {
        Ok(std::mem::replace(active, evaluations.to_vec()))
    })?;

    let _guard = LocalEvaluationStackGuard {
        previous: Some(previous),
    };

    operation()
}

pub(crate) fn current_cycle(
    runtime: RuntimeIdentity,
    key: &CompilationFactKey,
) -> Result<FactCycle, FactQueryError> {
    local_evaluations(TaskLocalOperation::Cycle, |active| {
        task_cycle(active, runtime, key).ok_or_else(|| {
            FactRuntimeFailure::MissingCycle {
                runtime,
                fact: key.clone(),
                active: active
                    .iter()
                    .filter(|context| context.data.runtime == runtime)
                    .map(|context| context.data.cycle_key.clone())
                    .collect(),
            }
            .into()
        })
    })
}

fn local_evaluations<T>(
    operation_kind: TaskLocalOperation,
    operation: impl FnOnce(&mut Vec<FactTaskContext>) -> Result<T, FactQueryError>,
) -> Result<T, FactQueryError> {
    LOCAL_EVALUATIONS
        .try_with(|active| {
            let mut active = active.try_borrow_mut().map_err(|_| {
                FactRuntimeFailure::TaskLocalStateUnavailable {
                    operation: operation_kind,
                    cause: LocalStateFailure::BorrowConflict,
                }
            })?;

            operation(&mut active)
        })
        .map_err(|_| FactRuntimeFailure::TaskLocalStateUnavailable {
            operation: operation_kind,
            cause: LocalStateFailure::Unavailable,
        })?
}

fn task_cycle(
    active: &[FactTaskContext],
    runtime: RuntimeIdentity,
    key: &CompilationFactKey,
) -> Option<FactCycle> {
    // Cycle reporting owns its path after the task-local stack is released.
    let facts = active
        .iter()
        .filter(|context| context.data.runtime == runtime)
        .map(|context| context.data.cycle_key.clone())
        .collect::<Vec<_>>();

    let start = facts.iter().position(|active_key| active_key == key)?;
    let mut cycle = facts[start..].to_vec();

    cycle.push(key.clone());

    Some(FactCycle::new(cycle))
}

struct LocalTaskGuard<'a> {
    context: &'a FactTaskContext,
}

impl Drop for LocalTaskGuard<'_> {
    fn drop(&mut self) {
        // Guard cleanup cannot return a task-local failure while unwinding the operation.
        let _ = local_evaluations(TaskLocalOperation::Enter, |active| {
            if active
                .last()
                .is_some_and(|context| Arc::ptr_eq(&context.data, &self.context.data))
            {
                active.pop();
            }

            Ok(())
        });
    }
}

struct LocalEvaluationStackGuard {
    previous: Option<Vec<FactTaskContext>>,
}

impl Drop for LocalEvaluationStackGuard {
    fn drop(&mut self) {
        let Some(previous) = self.previous.take() else {
            return;
        };

        // Guard cleanup cannot return a task-local failure while restoring the previous worker.
        let _ = local_evaluations(TaskLocalOperation::Replace, |active| {
            *active = previous;

            Ok(())
        });
    }
}

#[cfg(test)]
mod tests {
    use bray_source::SourceId;

    use super::{
        FactTaskContext, FactTaskIdentity, RuntimeIdentity, current_context, record_input,
    };
    use crate::fact::{
        CompilationFactKey, CompilationInputKey, FactQueryError, FactRuntimeFailure, FactTaskPhase,
        TaskOperation,
    };

    #[test]
    fn finished_tasks_report_the_exact_phase_and_operation() {
        let fact = CompilationFactKey::DeclarationTable;

        let context = FactTaskContext::with_cycle_key(
            RuntimeIdentity(1),
            FactTaskIdentity(2),
            fact.clone(),
            fact.clone(),
        );

        context
            .finish()
            .unwrap_or_else(|error| panic!("task should finish once: {error:?}"));

        let error = match context.finish() {
            Ok(_) => panic!("a finished task must reject another finish"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            FactQueryError::Runtime(error)
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::InvalidTaskState {
                        operation: TaskOperation::Finish,
                        expected: FactTaskPhase::Recording,
                        actual: FactTaskPhase::Finished,
                        task: FactTaskIdentity(2),
                        fact: CompilationFactKey::DeclarationTable,
                    }
                )
        ));
    }

    #[test]
    fn finished_tasks_reject_fixed_and_frozen_dependency_recording() {
        let fact = CompilationFactKey::DeclarationTable;

        let context = FactTaskContext::with_cycle_key(
            RuntimeIdentity(9),
            FactTaskIdentity(10),
            fact.clone(),
            fact,
        );

        context
            .finish()
            .unwrap_or_else(|error| panic!("task should finish once: {error:?}"));

        let fixed = context
            .record_fixed_input(1)
            .expect_err("a finished task must reject fixed-input recording");

        let frozen = context
            .record_frozen_fact(1)
            .expect_err("a finished task must reject frozen-fact recording");

        assert!(matches!(
            fixed,
            FactQueryError::Runtime(error)
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::InvalidTaskState {
                        operation: TaskOperation::RecordFixedInput,
                        actual: FactTaskPhase::Finished,
                        ..
                    }
                )
        ));

        assert!(matches!(
            frozen,
            FactQueryError::Runtime(error)
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::InvalidTaskState {
                        operation: TaskOperation::RecordFrozenFact,
                        actual: FactTaskPhase::Finished,
                        ..
                    }
                )
        ));
    }

    #[test]
    fn foreign_current_context_retains_both_runtime_identities() {
        let fact = CompilationFactKey::SyntaxTree;

        let context = FactTaskContext::with_cycle_key(
            RuntimeIdentity(3),
            FactTaskIdentity(4),
            fact.clone(),
            fact,
        );

        let error = match context.run(|| current_context(RuntimeIdentity(5)).map(|_| ())) {
            Ok(()) => panic!("a foreign runtime must not capture this task"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            FactQueryError::Runtime(error)
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::InvalidTaskContext {
                        expected_runtime: RuntimeIdentity(5),
                        actual,
                    } if actual.runtime == RuntimeIdentity(3)
                        && actual.task == FactTaskIdentity(4)
                        && actual.fact == CompilationFactKey::SyntaxTree
                )
        ));
    }

    #[test]
    fn repeated_input_fingerprints_must_be_stable() {
        let fact = CompilationFactKey::CheckDiagnostics;
        let input = CompilationInputKey::Source(SourceId::new(7));

        let context = FactTaskContext::with_cycle_key(
            RuntimeIdentity(6),
            FactTaskIdentity(8),
            fact.clone(),
            fact.clone(),
        );

        let first = crate::fact::fact_fingerprint(&fact, &1_u8);
        let changed = crate::fact::fact_fingerprint(&fact, &2_u8);
        context.record_input(&input, first).unwrap();
        context.record_input(&input, first).unwrap();

        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                context.record_input(&input, changed)
            }))
            .is_err()
        );
    }

    #[test]
    fn missing_input_fingerprints_retain_task_and_input_identity() {
        let fact = CompilationFactKey::CheckDiagnostics;
        let input = CompilationInputKey::Source(SourceId::new(7));

        let context = FactTaskContext::with_cycle_key(
            RuntimeIdentity(6),
            FactTaskIdentity(8),
            fact.clone(),
            fact,
        );

        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            context.run(|| record_input(RuntimeIdentity(6), &input, None))
        }))
        .expect_err("missing input identity is a compiler invariant");

        let message = panic
            .downcast_ref::<String>()
            .expect("contextual panic has a message");

        assert!(message.contains("CheckDiagnostics"));
        assert!(message.contains("Source"));
        assert!(message.contains("7"));
        assert!(message.contains("FactTaskIdentity(8)"));
    }
}
