use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque, hash_map::Entry};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use crate::WorkerBudget;

use super::scheduler::FactScheduler;
use super::task::{
    FactTaskContext, FactTaskIdentity, RuntimeIdentity, capture_evaluations, current_context,
    current_cycle, record_request_with_cycle_key, run_with_evaluations,
};
use super::{CompilationFactKey, FactCycle, FactQueryError};

#[derive(Debug)]
pub(crate) struct FactRuntime {
    next_task: AtomicU64,
    state: Mutex<RuntimeState>,
    scheduler: FactScheduler,
}

#[derive(Debug, Default)]
struct RuntimeState {
    owners: HashMap<CompilationFactKey, FactTaskIdentity>,
    waiting: HashMap<FactTaskIdentity, BTreeMap<WaitEdge, usize>>,
    dependencies: BTreeMap<CompilationFactKey, BTreeSet<CompilationFactKey>>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct WaitEdge {
    fact: CompilationFactKey,
    owner: FactTaskIdentity,
}

impl FactRuntime {
    pub(crate) fn new(worker_budget: WorkerBudget) -> Self {
        Self {
            next_task: AtomicU64::new(0),
            state: Mutex::new(RuntimeState::default()),
            scheduler: FactScheduler::new(worker_budget),
        }
    }

    pub(crate) fn updated(
        &self,
        worker_budget: WorkerBudget,
        invalidation_roots: impl IntoIterator<Item = CompilationFactKey>,
    ) -> (Self, BTreeSet<CompilationFactKey>) {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|_| panic!("fact dependency state must remain available"));

        let mut invalidated = invalidation_roots.into_iter().collect::<BTreeSet<_>>();
        let mut dependents = BTreeMap::<CompilationFactKey, Vec<CompilationFactKey>>::new();

        // The revised runtime owns stable graph keys independently of the previous snapshot.
        for (fact, dependencies) in &state.dependencies {
            for dependency in dependencies {
                dependents
                    .entry(dependency.clone())
                    .or_default()
                    .push(fact.clone());
            }
        }

        let mut pending = invalidated.iter().cloned().collect::<VecDeque<_>>();

        while let Some(invalidated_fact) = pending.pop_front() {
            let Some(affected) = dependents.get(&invalidated_fact) else {
                continue;
            };

            for fact in affected {
                if invalidated.insert(fact.clone()) {
                    pending.push_back(fact.clone());
                }
            }
        }

        // Retained dependency sets remain independently owned after the previous runtime is gone.
        let dependencies = state
            .dependencies
            .iter()
            .filter(|(fact, _)| !invalidated.contains(*fact))
            .map(|(fact, dependencies)| (fact.clone(), dependencies.clone()))
            .collect::<BTreeMap<_, _>>();

        let reusable = dependencies.keys().cloned().collect();
        let runtime = Self {
            next_task: AtomicU64::new(0),
            state: Mutex::new(RuntimeState {
                dependencies,
                ..RuntimeState::default()
            }),
            scheduler: FactScheduler::new(worker_budget),
        };

        (runtime, reusable)
    }

    pub(crate) fn run<T>(
        &self,
        operation: impl FnOnce() -> Result<T, FactQueryError> + Send,
    ) -> Result<T, FactQueryError>
    where
        T: Send,
    {
        self.scheduler.run(operation)?
    }

    pub(crate) fn map_indexed<T>(
        &self,
        len: usize,
        operation: impl Fn(usize) -> T + Send + Sync,
    ) -> Result<Vec<T>, FactQueryError>
    where
        T: Send,
    {
        let evaluations = capture_evaluations()?;

        let results = self.scheduler.map_indexed(len, |index| {
            run_with_evaluations(&evaluations, || Ok(operation(index)))
        })?;

        results.into_iter().collect()
    }

    pub(crate) fn request_with_cycle_key(
        &self,
        key: &CompilationFactKey,
        cycle_key: &CompilationFactKey,
    ) -> Result<(), FactQueryError> {
        record_request_with_cycle_key(self.identity(), key, cycle_key)
    }

    pub(crate) fn current_task_context(&self) -> Result<FactTaskContext, FactQueryError> {
        current_context(self.identity())
    }

    pub(crate) fn task_with_cycle_key(
        &self,
        key: CompilationFactKey,
        cycle_key: CompilationFactKey,
    ) -> Result<FactTaskContext, FactQueryError> {
        let identity = self
            .next_task
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map(FactTaskIdentity)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        Ok(FactTaskContext::with_cycle_key(
            self.identity(),
            identity,
            key,
            cycle_key,
        ))
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

    pub(crate) fn same_task_cycle(
        &self,
        key: &CompilationFactKey,
    ) -> Result<FactCycle, FactQueryError> {
        current_cycle(self.identity(), key)
    }

    pub(crate) fn wait_for(
        &self,
        key: &CompilationFactKey,
        observed_owner: FactTaskIdentity,
    ) -> Result<Option<WaitingGuard<'_>>, FactQueryError> {
        let requester = self.current_task_context().ok();

        let mut state = self.state()?;

        let Some(owner) = state.owners.get(key).copied() else {
            return Ok(None);
        };

        if owner != observed_owner {
            return Ok(None);
        }

        // The guard owns the exact observed edge after the caller releases its cache lock.
        let edge = WaitEdge {
            fact: key.clone(),
            owner,
        };

        let Some(requester) = requester else {
            return Ok(Some(WaitingGuard {
                runtime: self,
                task: None,
                edge,
            }));
        };

        let requester_identity = requester.identity();

        // The registry and guard retain independent edge keys while the wait is active.
        let waiting = state.waiting.entry(requester_identity).or_default();
        let count = waiting.entry(edge.clone()).or_default();

        *count = count
            .checked_add(1)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        // Cycle reporting owns its root key after the runtime lock is released.
        let cycle = cross_task_cycle(
            &state,
            requester_identity,
            requester.key().clone(),
            owner,
            key,
        );

        let cycle = match cycle {
            Ok(cycle) => cycle,
            Err(error) => {
                remove_wait(&mut state, requester_identity, &edge);

                return Err(error);
            }
        };

        if let Some(cycle) = cycle {
            remove_wait(&mut state, requester_identity, &edge);

            return Err(FactQueryError::Cycle(cycle));
        }

        Ok(Some(WaitingGuard {
            runtime: self,
            task: Some(requester_identity),
            edge,
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

    fn finish_waiting(&self, task: FactTaskIdentity, edge: &WaitEdge) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        remove_wait(&mut state, task, edge);
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

#[cfg(test)]
impl Default for FactRuntime {
    fn default() -> Self {
        Self::new(WorkerBudget::serial())
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

        for edge in waited_keys.keys() {
            if state.owners.get(&edge.fact).copied() != Some(edge.owner) {
                continue;
            }

            let next_owner = edge.owner;

            if !visited.insert(next_owner) {
                continue;
            }

            // Predecessors retain graph edges while the breadth-first search continues.
            predecessors.insert(next_owner, (current, edge.fact.clone()));

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

fn remove_wait(state: &mut RuntimeState, task: FactTaskIdentity, edge: &WaitEdge) {
    let remove_task = match state.waiting.get_mut(&task) {
        Some(waiting) => {
            let remove_edge = match waiting.get_mut(edge) {
                Some(count) if *count > 1 => {
                    *count -= 1;

                    false
                }
                Some(_) => true,
                None => false,
            };

            if remove_edge {
                waiting.remove(edge);
            }

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
    edge: WaitEdge,
}

impl Drop for WaitingGuard<'_> {
    fn drop(&mut self) {
        if let Some(task) = self.task {
            self.runtime.finish_waiting(task, &self.edge);
        }
    }
}
