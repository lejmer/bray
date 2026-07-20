use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, hash_map::Entry};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, ThreadId};

use super::{CompilationFactKey, FactCycle, FactQueryError};

thread_local! {
    static LOCAL_EVALUATIONS: RefCell<Vec<FactTaskContext>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug, Default)]
pub(crate) struct FactRuntime {
    state: Mutex<RuntimeState>,
}

#[derive(Debug, Default)]
struct RuntimeState {
    active: HashMap<ThreadId, Vec<CompilationFactKey>>,
    owners: HashMap<CompilationFactKey, ThreadId>,
    waiting: HashMap<ThreadId, CompilationFactKey>,
    dependencies: BTreeMap<CompilationFactKey, BTreeSet<CompilationFactKey>>,
}

#[derive(Clone, Debug)]
pub(crate) struct FactTaskContext {
    data: Arc<FactTaskData>,
}

#[derive(Debug)]
struct FactTaskData {
    runtime: RuntimeIdentity,
    key: CompilationFactKey,
    state: Mutex<FactTaskState>,
}

#[derive(Debug)]
struct FactTaskState {
    accepting_dependencies: bool,
    dependencies: BTreeSet<CompilationFactKey>,
}

impl FactTaskContext {
    fn new(runtime: RuntimeIdentity, key: CompilationFactKey) -> Self {
        Self {
            data: Arc::new(FactTaskData {
                runtime,
                key,
                state: Mutex::new(FactTaskState {
                    accepting_dependencies: true,
                    dependencies: BTreeSet::new(),
                }),
            }),
        }
    }

    fn record(&self, key: &CompilationFactKey) -> Result<(), FactQueryError> {
        let mut state = self
            .data
            .state
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        if !state.accepting_dependencies {
            return Err(FactQueryError::InfrastructureFailure);
        }

        // The dependency graph must own its keys after the accessor returns.
        state.dependencies.insert(key.clone());

        Ok(())
    }

    fn finish(&self) -> Result<BTreeSet<CompilationFactKey>, FactQueryError> {
        let mut state = self
            .data
            .state
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        if !state.accepting_dependencies {
            return Err(FactQueryError::InfrastructureFailure);
        }

        state.accepting_dependencies = false;

        Ok(std::mem::take(&mut state.dependencies))
    }

    fn discard(&self) {
        let Ok(mut state) = self.data.state.lock() else {
            return;
        };

        state.accepting_dependencies = false;
        state.dependencies.clear();
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RuntimeIdentity(usize);

impl FactRuntime {
    pub(crate) fn request(&self, key: &CompilationFactKey) -> Result<(), FactQueryError> {
        local_evaluations(|active| {
            let Some(context) = active.last() else {
                return Ok(());
            };

            if context.data.runtime != self.identity() {
                return Ok(());
            }

            if let Some(start) = active.iter().position(|context| {
                context.data.runtime == self.identity() && &context.data.key == key
            }) {
                // Cycle reporting owns its path after the task-local stack is released.
                let mut facts = active[start..]
                    .iter()
                    .map(|context| context.data.key.clone())
                    .collect::<Vec<_>>();

                facts.push(key.clone());

                return Err(FactQueryError::Cycle(FactCycle::new(facts)));
            }

            context.record(key)
        })
    }

    // TODO(compilation): Remove this allow when BRA-228 propagates contexts in the scheduler.
    #[allow(dead_code)]
    pub(crate) fn current_task_context(&self) -> Result<FactTaskContext, FactQueryError> {
        local_evaluations(|active| {
            let Some(context) = active.last() else {
                return Err(FactQueryError::InfrastructureFailure);
            };

            if context.data.runtime != self.identity() {
                return Err(FactQueryError::InfrastructureFailure);
            }

            // Scoped workers share dependency state without sharing their local context stacks.
            Ok(context.clone())
        })
    }

    pub(crate) fn begin(
        &self,
        key: CompilationFactKey,
    ) -> Result<EvaluationGuard<'_>, FactQueryError> {
        let thread = thread::current().id();
        let mut state = self.state()?;

        // Each runtime record owns its key so no caller or cache lock must remain held.
        match state.owners.entry(key.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(thread);
            }
            Entry::Occupied(_) => return Err(FactQueryError::InfrastructureFailure),
        }

        state.active.entry(thread).or_default().push(key.clone());

        let context = FactTaskContext::new(self.identity(), key.clone());

        Ok(EvaluationGuard {
            runtime: self,
            thread,
            key,
            context,
            active: true,
        })
    }

    pub(crate) fn same_thread_cycle(
        &self,
        key: CompilationFactKey,
    ) -> Result<FactCycle, FactQueryError> {
        let thread = thread::current().id();
        let state = self.state()?;

        let active = state
            .active
            .get(&thread)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let start = active
            .iter()
            .position(|active_key| active_key == &key)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let mut facts = active[start..].to_vec();

        facts.push(key);

        Ok(FactCycle::new(facts))
    }

    pub(crate) fn wait_for(
        &self,
        key: CompilationFactKey,
        owner: ThreadId,
    ) -> Result<WaitingGuard<'_>, FactQueryError> {
        let thread = thread::current().id();

        if thread == owner {
            let cycle = self.same_thread_cycle(key)?;

            return Err(FactQueryError::Cycle(cycle));
        }

        let mut state = self.state()?;

        match state.waiting.entry(thread) {
            Entry::Vacant(entry) => {
                entry.insert(key.clone());
            }
            Entry::Occupied(_) => return Err(FactQueryError::InfrastructureFailure),
        }

        if let Some(cycle) = cross_thread_cycle(&state, thread, owner, &key)? {
            state.waiting.remove(&thread);

            return Err(FactQueryError::Cycle(cycle));
        }

        Ok(WaitingGuard {
            runtime: self,
            thread,
            key,
        })
    }

    fn state(&self) -> Result<MutexGuard<'_, RuntimeState>, FactQueryError> {
        self.state
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)
    }

    fn identity(&self) -> RuntimeIdentity {
        RuntimeIdentity(std::ptr::from_ref(self).addr())
    }

    fn prepare_evaluation<'runtime>(
        &'runtime self,
        thread: ThreadId,
        key: &CompilationFactKey,
        context: &FactTaskContext,
    ) -> Result<EvaluationCommit<'runtime>, FactQueryError> {
        let mut state = self.state()?;

        if state.owners.get(key) != Some(&thread) {
            return Err(FactQueryError::InfrastructureFailure);
        }

        let active = state
            .active
            .get_mut(&thread)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        if active.last() != Some(key) {
            return Err(FactQueryError::InfrastructureFailure);
        }

        let dependencies = context.finish()?;

        // The commit owns the key while coordinating runtime and cache publication locks.
        Ok(EvaluationCommit {
            state,
            thread,
            key: Some(key.clone()),
            dependencies,
        })
    }

    fn abandon_evaluation(&self, thread: ThreadId, key: &CompilationFactKey) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        remove_evaluation(&mut state, thread, key);
    }

    fn finish_waiting(&self, thread: ThreadId, key: &CompilationFactKey) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        if state.waiting.get(&thread) == Some(key) {
            state.waiting.remove(&thread);
        }
    }

    #[cfg(test)]
    pub(crate) fn dependencies(
        &self,
        key: &CompilationFactKey,
    ) -> Result<Option<Box<[CompilationFactKey]>>, FactQueryError> {
        let state = self.state()?;

        Ok(state
            .dependencies
            .get(key)
            .map(|dependencies| dependencies.iter().cloned().collect::<Box<[_]>>()))
    }
}

fn cross_thread_cycle(
    state: &RuntimeState,
    requesting_thread: ThreadId,
    owner: ThreadId,
    requested_key: &CompilationFactKey,
) -> Result<Option<FactCycle>, FactQueryError> {
    // Cycle reporting owns its path after the runtime lock is released.
    let mut facts = state
        .active
        .get(&requesting_thread)
        .cloned()
        .unwrap_or_default();

    facts.push(requested_key.clone());

    let mut thread = owner;
    let mut visited = HashSet::new();

    while visited.insert(thread) {
        let Some(waited_key) = state.waiting.get(&thread).cloned() else {
            return Ok(None);
        };

        facts.push(waited_key.clone());

        let Some(next_owner) = state.owners.get(&waited_key).copied() else {
            return Ok(None);
        };

        if next_owner == requesting_thread {
            return Ok(Some(FactCycle::new(facts)));
        }

        thread = next_owner;
    }

    Ok(Some(FactCycle::new(facts)))
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

fn remove_evaluation(state: &mut RuntimeState, thread: ThreadId, key: &CompilationFactKey) {
    if state.owners.get(key) == Some(&thread) {
        state.owners.remove(key);
    }

    let remove_stack = match state.active.get_mut(&thread) {
        Some(active) => {
            if active.last() == Some(key) {
                active.pop();
            }

            active.is_empty()
        }
        None => false,
    };

    if remove_stack {
        state.active.remove(&thread);
    }
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

pub(crate) struct EvaluationGuard<'a> {
    runtime: &'a FactRuntime,
    thread: ThreadId,
    key: CompilationFactKey,
    context: FactTaskContext,
    active: bool,
}

impl<'a> EvaluationGuard<'a> {
    pub(crate) fn run<T>(
        &self,
        operation: impl FnOnce() -> Result<T, FactQueryError>,
    ) -> Result<T, FactQueryError> {
        self.context.run(operation)
    }

    pub(crate) fn prepare(mut self) -> Result<EvaluationCommit<'a>, FactQueryError> {
        let commit = self
            .runtime
            .prepare_evaluation(self.thread, &self.key, &self.context)?;

        self.active = false;

        Ok(commit)
    }
}

impl Drop for EvaluationGuard<'_> {
    fn drop(&mut self) {
        if self.active {
            self.context.discard();
            self.runtime.abandon_evaluation(self.thread, &self.key);
        }
    }
}

pub(crate) struct EvaluationCommit<'a> {
    state: MutexGuard<'a, RuntimeState>,
    thread: ThreadId,
    key: Option<CompilationFactKey>,
    dependencies: BTreeSet<CompilationFactKey>,
}

impl EvaluationCommit<'_> {
    pub(crate) fn commit(mut self) {
        let Some(key) = self.key.take() else {
            return;
        };

        remove_evaluation(&mut self.state, self.thread, &key);

        self.state
            .dependencies
            .insert(key, std::mem::take(&mut self.dependencies));
    }
}

impl Drop for EvaluationCommit<'_> {
    fn drop(&mut self) {
        let Some(key) = self.key.take() else {
            return;
        };

        remove_evaluation(&mut self.state, self.thread, &key);
    }
}

pub(crate) struct WaitingGuard<'a> {
    runtime: &'a FactRuntime,
    thread: ThreadId,
    key: CompilationFactKey,
}

impl Drop for WaitingGuard<'_> {
    fn drop(&mut self) {
        self.runtime.finish_waiting(self.thread, &self.key);
    }
}
