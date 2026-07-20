use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque, hash_map::Entry};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use super::task::{
    FactTaskContext, FactTaskIdentity, RuntimeIdentity, current_context, current_cycle,
    record_request,
};
use super::{CompilationFactKey, FactCycle, FactQueryError};

#[derive(Debug, Default)]
pub(crate) struct FactRuntime {
    next_task: AtomicU64,
    state: Mutex<RuntimeState>,
}

#[derive(Debug, Default)]
struct RuntimeState {
    owners: HashMap<CompilationFactKey, FactTaskIdentity>,
    waiting: HashMap<FactTaskIdentity, BTreeSet<CompilationFactKey>>,
    dependencies: BTreeMap<CompilationFactKey, BTreeSet<CompilationFactKey>>,
}

impl FactRuntime {
    pub(crate) fn request(&self, key: &CompilationFactKey) -> Result<(), FactQueryError> {
        record_request(self.identity(), key)
    }

    pub(crate) fn current_task_context(&self) -> Result<FactTaskContext, FactQueryError> {
        current_context(self.identity())
    }

    pub(crate) fn task(&self, key: CompilationFactKey) -> Result<FactTaskContext, FactQueryError> {
        let identity = self
            .next_task
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map(FactTaskIdentity)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        Ok(FactTaskContext::new(self.identity(), identity, key))
    }

    pub(crate) fn begin(
        &self,
        context: FactTaskContext,
    ) -> Result<EvaluationGuard<'_>, FactQueryError> {
        // Runtime ownership outlives this borrowed view of the shared task context.
        let key = context.key().clone();
        let task = context.identity();

        let mut state = self.state()?;

        // The owner record and evaluation guard retain independent keys after this lock is released.
        match state.owners.entry(key.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(task);
            }
            Entry::Occupied(_) => return Err(FactQueryError::InfrastructureFailure),
        }

        Ok(EvaluationGuard {
            runtime: self,
            key,
            context,
            active: true,
        })
    }

    pub(crate) fn same_thread_cycle(
        &self,
        key: CompilationFactKey,
    ) -> Result<FactCycle, FactQueryError> {
        current_cycle(self.identity(), &key)
    }

    pub(crate) fn wait_for(
        &self,
        key: CompilationFactKey,
        observed_owner: FactTaskIdentity,
    ) -> Result<Option<WaitingGuard<'_>>, FactQueryError> {
        let requester = self.current_task_context().ok();

        let mut state = self.state()?;

        let Some(owner) = state.owners.get(&key).copied() else {
            return Ok(None);
        };

        if owner != observed_owner {
            return Ok(None);
        }

        let Some(requester) = requester else {
            return Ok(Some(WaitingGuard {
                runtime: self,
                task: None,
                key,
            }));
        };

        let requester_identity = requester.identity();

        // The wait edge remains registered after the caller's key moves into its guard.
        state
            .waiting
            .entry(requester_identity)
            .or_default()
            .insert(key.clone());

        // Cycle reporting owns its root key after the runtime lock is released.
        if let Some(cycle) = cross_task_cycle(
            &state,
            requester_identity,
            requester.key().clone(),
            owner,
            &key,
        )? {
            remove_wait(&mut state, requester_identity, &key);

            return Err(FactQueryError::Cycle(cycle));
        }

        Ok(Some(WaitingGuard {
            runtime: self,
            task: Some(requester_identity),
            key,
        }))
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
        key: &CompilationFactKey,
        context: &FactTaskContext,
    ) -> Result<EvaluationCommit<'runtime>, FactQueryError> {
        let state = self.state()?;

        if state.owners.get(key).copied() != Some(context.identity()) {
            return Err(FactQueryError::InfrastructureFailure);
        }

        let dependencies = context.finish()?;

        // The commit owns the key while coordinating runtime and cache publication locks.
        Ok(EvaluationCommit {
            state,
            task: context.identity(),
            key: Some(key.clone()),
            dependencies,
        })
    }

    fn abandon_evaluation(&self, context: &FactTaskContext, key: &CompilationFactKey) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        remove_evaluation(&mut state, context.identity(), key);
    }

    fn finish_waiting(&self, task: FactTaskIdentity, key: &CompilationFactKey) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        remove_wait(&mut state, task, key);
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

fn cross_task_cycle(
    state: &RuntimeState,
    requester: FactTaskIdentity,
    requester_key: CompilationFactKey,
    owner: FactTaskIdentity,
    requested_key: &CompilationFactKey,
) -> Result<Option<FactCycle>, FactQueryError> {
    // Cycle reporting owns its path after the runtime lock is released.
    let mut facts = vec![requester_key, requested_key.clone()];

    if owner == requester {
        return Ok(Some(FactCycle::new(facts)));
    }

    let mut pending = VecDeque::from([owner]);
    let mut visited = HashSet::from([owner]);
    let mut predecessors = HashMap::new();

    while let Some(current) = pending.pop_front() {
        let Some(waited_keys) = state.waiting.get(&current) else {
            continue;
        };

        for waited_key in waited_keys {
            let Some(next_owner) = state.owners.get(waited_key).copied() else {
                continue;
            };

            if !visited.insert(next_owner) {
                continue;
            }

            // Predecessors retain graph edges while the breadth-first search continues.
            predecessors.insert(next_owner, (current, waited_key.clone()));

            if next_owner == requester {
                let mut path = Vec::new();
                let mut cursor = requester;

                while cursor != owner {
                    let Some((previous, edge)) = predecessors.get(&cursor) else {
                        return Err(FactQueryError::InfrastructureFailure);
                    };

                    path.push(edge.clone());
                    cursor = *previous;
                }

                path.reverse();
                facts.extend(path);

                return Ok(Some(FactCycle::new(facts)));
            }

            pending.push_back(next_owner);
        }
    }

    Ok(None)
}

fn remove_evaluation(state: &mut RuntimeState, task: FactTaskIdentity, key: &CompilationFactKey) {
    if state.owners.get(key).copied() == Some(task) {
        state.owners.remove(key);
    }

    state.waiting.remove(&task);
}

fn remove_wait(state: &mut RuntimeState, task: FactTaskIdentity, key: &CompilationFactKey) {
    let remove_task = match state.waiting.get_mut(&task) {
        Some(waiting) => {
            waiting.remove(key);
            waiting.is_empty()
        }
        None => false,
    };

    if remove_task {
        state.waiting.remove(&task);
    }
}

pub(crate) struct EvaluationGuard<'a> {
    runtime: &'a FactRuntime,
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
        let commit = self.runtime.prepare_evaluation(&self.key, &self.context)?;

        self.active = false;

        Ok(commit)
    }
}

impl Drop for EvaluationGuard<'_> {
    fn drop(&mut self) {
        if self.active {
            self.context.discard();
            self.runtime.abandon_evaluation(&self.context, &self.key);
        }
    }
}

pub(crate) struct EvaluationCommit<'a> {
    state: MutexGuard<'a, RuntimeState>,
    task: FactTaskIdentity,
    key: Option<CompilationFactKey>,
    dependencies: BTreeSet<CompilationFactKey>,
}

impl EvaluationCommit<'_> {
    pub(crate) fn commit(mut self) {
        let Some(key) = self.key.take() else {
            return;
        };

        remove_evaluation(&mut self.state, self.task, &key);

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

        remove_evaluation(&mut self.state, self.task, &key);
    }
}

pub(crate) struct WaitingGuard<'a> {
    runtime: &'a FactRuntime,
    task: Option<FactTaskIdentity>,
    key: CompilationFactKey,
}

impl Drop for WaitingGuard<'_> {
    fn drop(&mut self) {
        if let Some(task) = self.task {
            self.runtime.finish_waiting(task, &self.key);
        }
    }
}
