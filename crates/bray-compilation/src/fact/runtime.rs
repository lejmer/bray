use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, hash_map::Entry};
use std::sync::{Mutex, MutexGuard};
use std::thread::{self, ThreadId};

use super::{CompilationFactKey, FactCycle, FactQueryError};

thread_local! {
    static LOCAL_EVALUATIONS: RefCell<Vec<LocalEvaluation>> = const { RefCell::new(Vec::new()) };
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

#[derive(Debug)]
struct LocalEvaluation {
    runtime: RuntimeIdentity,
    dependencies: BTreeSet<CompilationFactKey>,
}

impl LocalEvaluation {
    fn new(runtime: RuntimeIdentity) -> Self {
        Self {
            runtime,
            dependencies: BTreeSet::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RuntimeIdentity(usize);

impl FactRuntime {
    pub(crate) fn request(&self, key: &CompilationFactKey) -> Result<(), FactQueryError> {
        self.local_evaluations(|active| {
            let Some(evaluation) = active.last_mut() else {
                return Ok(());
            };

            if evaluation.runtime != self.identity() {
                return Ok(());
            }

            // The dependency graph must own its keys after the accessor returns.
            evaluation.dependencies.insert(key.clone());

            Ok(())
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

        if let Err(error) = self.push_local_evaluation() {
            state.owners.remove(&key);

            if let Some(active) = state.active.get_mut(&thread) {
                active.pop();

                if active.is_empty() {
                    state.active.remove(&thread);
                }
            }

            return Err(error);
        }

        Ok(EvaluationGuard {
            runtime: self,
            thread,
            key,
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

    fn local_evaluations<T>(
        &self,
        operation: impl FnOnce(&mut Vec<LocalEvaluation>) -> Result<T, FactQueryError>,
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

    fn push_local_evaluation(&self) -> Result<(), FactQueryError> {
        self.local_evaluations(|active| {
            active.push(LocalEvaluation::new(self.identity()));

            Ok(())
        })
    }

    fn take_local_dependencies(&self) -> Result<BTreeSet<CompilationFactKey>, FactQueryError> {
        self.local_evaluations(|active| {
            if active.last().map(|evaluation| evaluation.runtime) != Some(self.identity()) {
                return Err(FactQueryError::InfrastructureFailure);
            }

            let Some(evaluation) = active.pop() else {
                return Err(FactQueryError::InfrastructureFailure);
            };

            Ok(evaluation.dependencies)
        })
    }

    fn discard_local_evaluation(&self) {
        let _ = self.local_evaluations(|active| {
            if active.last().map(|evaluation| evaluation.runtime) == Some(self.identity()) {
                active.pop();
            }

            Ok(())
        });
    }

    fn complete_evaluation(
        &self,
        thread: ThreadId,
        key: &CompilationFactKey,
    ) -> Result<(), FactQueryError> {
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

        let dependencies = self.take_local_dependencies()?;

        active.pop();

        let remove_stack = active.is_empty();

        state.owners.remove(key);

        // The committed graph remains available after this evaluation record is removed.
        state.dependencies.insert(key.clone(), dependencies);

        if remove_stack {
            state.active.remove(&thread);
        }

        Ok(())
    }

    fn finish_evaluation(&self, thread: ThreadId, key: &CompilationFactKey) {
        self.discard_local_evaluation();

        let Ok(mut state) = self.state.lock() else {
            return;
        };

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
    ) -> Result<Box<[CompilationFactKey]>, FactQueryError> {
        let state = self.state()?;

        Ok(state
            .dependencies
            .get(key)
            .into_iter()
            .flatten()
            .cloned()
            .collect())
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

pub(crate) struct EvaluationGuard<'a> {
    runtime: &'a FactRuntime,
    thread: ThreadId,
    key: CompilationFactKey,
    active: bool,
}

impl EvaluationGuard<'_> {
    pub(crate) fn complete(mut self) -> Result<(), FactQueryError> {
        self.runtime.complete_evaluation(self.thread, &self.key)?;
        self.active = false;

        Ok(())
    }
}

impl Drop for EvaluationGuard<'_> {
    fn drop(&mut self) {
        if self.active {
            self.runtime.finish_evaluation(self.thread, &self.key);
        }
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
