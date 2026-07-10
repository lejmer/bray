use std::collections::{HashMap, HashSet, hash_map::Entry};
use std::sync::{Mutex, MutexGuard};
use std::thread::{self, ThreadId};

use super::{CompilationFactKey, FactCycle, FactQueryError};

#[derive(Debug, Default)]
pub(crate) struct FactRuntime {
    state: Mutex<RuntimeState>,
}

#[derive(Debug, Default)]
struct RuntimeState {
    active: HashMap<ThreadId, Vec<CompilationFactKey>>,
    owners: HashMap<CompilationFactKey, ThreadId>,
    waiting: HashMap<ThreadId, CompilationFactKey>,
}

impl FactRuntime {
    pub(crate) fn begin(
        &self,
        key: CompilationFactKey,
    ) -> Result<EvaluationGuard<'_>, FactQueryError> {
        let thread = thread::current().id();
        let mut state = self.state()?;

        match state.owners.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(thread);
            }
            Entry::Occupied(_) => return Err(FactQueryError::InfrastructureFailure),
        }

        state.active.entry(thread).or_default().push(key);

        Ok(EvaluationGuard {
            runtime: self,
            thread,
            key,
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
            .position(|active_key| *active_key == key)
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
                entry.insert(key);
            }
            Entry::Occupied(_) => return Err(FactQueryError::InfrastructureFailure),
        }

        if let Some(cycle) = cross_thread_cycle(&state, thread, owner, key)? {
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

    fn finish_evaluation(&self, thread: ThreadId, key: CompilationFactKey) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        if state.owners.get(&key) == Some(&thread) {
            state.owners.remove(&key);
        }

        let remove_stack = match state.active.get_mut(&thread) {
            Some(active) => {
                if active.last() == Some(&key) {
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

    fn finish_waiting(&self, thread: ThreadId, key: CompilationFactKey) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        if state.waiting.get(&thread) == Some(&key) {
            state.waiting.remove(&thread);
        }
    }
}

fn cross_thread_cycle(
    state: &RuntimeState,
    requesting_thread: ThreadId,
    owner: ThreadId,
    requested_key: CompilationFactKey,
) -> Result<Option<FactCycle>, FactQueryError> {
    // Cycle reporting owns its path after the runtime lock is released.
    let mut facts = state
        .active
        .get(&requesting_thread)
        .cloned()
        .unwrap_or_default();

    facts.push(requested_key);

    let mut thread = owner;
    let mut visited = HashSet::new();

    while visited.insert(thread) {
        let Some(waited_key) = state.waiting.get(&thread).copied() else {
            return Ok(None);
        };

        facts.push(waited_key);

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
}

impl Drop for EvaluationGuard<'_> {
    fn drop(&mut self) {
        self.runtime.finish_evaluation(self.thread, self.key);
    }
}

pub(crate) struct WaitingGuard<'a> {
    runtime: &'a FactRuntime,
    thread: ThreadId,
    key: CompilationFactKey,
}

impl Drop for WaitingGuard<'_> {
    fn drop(&mut self) {
        self.runtime.finish_waiting(self.thread, self.key);
    }
}
