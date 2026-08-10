use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use super::{CompilationFactKey, CompilationInputKey, FactCycle, FactFingerprint, FactQueryError};

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
    state: Mutex<FactTaskState>,
}

#[derive(Debug)]
struct FactTaskState {
    accepting_dependencies: bool,
    dependencies: BTreeSet<CompilationFactKey>,
    inputs: BTreeMap<CompilationInputKey, FactFingerprint>,
}

pub(crate) struct RecordedDependencies {
    pub(crate) facts: BTreeSet<CompilationFactKey>,
    pub(crate) inputs: BTreeMap<CompilationInputKey, FactFingerprint>,
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
                state: Mutex::new(FactTaskState {
                    accepting_dependencies: true,
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

        if !state.accepting_dependencies {
            return Err(FactQueryError::InfrastructureFailure);
        }

        state.accepting_dependencies = false;

        Ok(RecordedDependencies {
            facts: std::mem::take(&mut state.dependencies),
            inputs: std::mem::take(&mut state.inputs),
        })
    }

    pub(crate) fn discard(&self) {
        let Ok(mut state) = self.data.state.lock() else {
            return;
        };

        state.accepting_dependencies = false;
        state.dependencies.clear();
        state.inputs.clear();
    }

    pub(crate) fn run<T>(
        &self,
        operation: impl FnOnce() -> Result<T, FactQueryError>,
    ) -> Result<T, FactQueryError> {
        local_evaluations(|active| {
            // Worker-local stacks share the task state while retaining independent stack storage.
            active.push(self.clone());

            Ok(())
        })?;

        let _guard = LocalTaskGuard { context: self };

        operation()
    }

    fn record(&self, key: &CompilationFactKey) -> Result<(), FactQueryError> {
        let mut state = self.state()?;

        if !state.accepting_dependencies {
            return Err(FactQueryError::InfrastructureFailure);
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

        if !state.accepting_dependencies {
            return Err(FactQueryError::InfrastructureFailure);
        }

        match state.inputs.insert(key.clone(), fingerprint) {
            Some(previous) if previous != fingerprint => Err(FactQueryError::InfrastructureFailure),
            _ => Ok(()),
        }
    }

    fn state(&self) -> Result<std::sync::MutexGuard<'_, FactTaskState>, FactQueryError> {
        self.data
            .state
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RuntimeIdentity(pub(crate) usize);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct FactTaskIdentity(pub(crate) u64);

pub(crate) fn record_request_with_cycle_key(
    runtime: RuntimeIdentity,
    key: &CompilationFactKey,
    cycle_key: &CompilationFactKey,
) -> Result<(), FactQueryError> {
    local_evaluations(|active| {
        let Some(context) = active.last() else {
            return Ok(());
        };

        if context.data.runtime != runtime {
            return Ok(());
        }

        if let Some(cycle) = task_cycle(active, runtime, cycle_key) {
            return Err(FactQueryError::Cycle(cycle));
        }

        context.record(key)
    })
}

pub(crate) fn record_input(
    runtime: RuntimeIdentity,
    key: &CompilationInputKey,
    fingerprint: Option<FactFingerprint>,
) -> Result<(), FactQueryError> {
    local_evaluations(|active| {
        let Some(context) = active.last() else {
            return Ok(());
        };

        if context.data.runtime != runtime {
            return Ok(());
        }

        context.record_input(
            key,
            fingerprint.ok_or(FactQueryError::InfrastructureFailure)?,
        )
    })
}

pub(crate) fn current_context(runtime: RuntimeIdentity) -> Result<FactTaskContext, FactQueryError> {
    local_evaluations(|active| {
        let Some(context) = active.last() else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        if context.data.runtime != runtime {
            return Err(FactQueryError::InfrastructureFailure);
        }

        // Scoped workers share dependency state without sharing their local context stacks.
        Ok(context.clone())
    })
}

pub(crate) fn capture_evaluations() -> Result<Vec<FactTaskContext>, FactQueryError> {
    // Scheduled jobs need independent stack storage while sharing each task's Arc-backed state.
    local_evaluations(|active| Ok(active.clone()))
}

pub(crate) fn run_with_evaluations<T>(
    evaluations: &[FactTaskContext],
    operation: impl FnOnce() -> Result<T, FactQueryError>,
) -> Result<T, FactQueryError> {
    // Each worker owns its local stack while retaining the caller's Arc-backed task contexts.
    let previous = local_evaluations(|active| Ok(std::mem::replace(active, evaluations.to_vec())))?;

    let _guard = LocalEvaluationStackGuard {
        previous: Some(previous),
    };

    operation()
}

pub(crate) fn current_cycle(
    runtime: RuntimeIdentity,
    key: &CompilationFactKey,
) -> Result<FactCycle, FactQueryError> {
    local_evaluations(|active| {
        task_cycle(active, runtime, key).ok_or(FactQueryError::InfrastructureFailure)
    })
}

fn local_evaluations<T>(
    operation: impl FnOnce(&mut Vec<FactTaskContext>) -> Result<T, FactQueryError>,
) -> Result<T, FactQueryError> {
    LOCAL_EVALUATIONS
        .try_with(|active| {
            let mut active = active
                .try_borrow_mut()
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            operation(&mut active)
        })
        .map_err(|_| FactQueryError::InfrastructureFailure)?
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
        let _ = local_evaluations(|active| {
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

        let _ = local_evaluations(|active| {
            *active = previous;

            Ok(())
        });
    }
}
