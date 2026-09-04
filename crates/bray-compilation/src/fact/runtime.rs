use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque, hash_map::Entry};
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::WorkerBudget;
use crate::profile::{
    CompilationProfileConfiguration, CompilationProfileContext, CompilationProfileReport,
    ProfileQueryKind, ProfileSession,
};
#[cfg(test)]
use std::fmt;

use super::scheduler::FactScheduler;
use super::task::{
    FactTaskContext, FactTaskIdentity, RuntimeIdentity, capture_evaluations, check_request_cycle,
    current_context, current_cycle, record_completed_request, record_frozen_fact, record_input,
    run_with_evaluations,
};
use super::{
    CapacityResource, CompilationFactKey, CompilationInputKey, CompilationInputs, FactCycle,
    FactDependencyRecord, FactQueryError, FactRuntimeFailure, PublicationIdentity,
    PublicationState, QueryPriority, QueryPriorityDemand, SynchronizationComponent,
    fact_fingerprint,
};

#[derive(Debug)]
pub(crate) struct FactRuntime {
    next_task: AtomicU64,
    state: Mutex<RuntimeState>,
    inputs: CompilationInputs,
    scheduler: FactScheduler,
    profile: Option<Arc<ProfileSession>>,
    #[cfg(test)]
    observer: Mutex<Option<FactEvaluationTestObserver>>,
}

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct FactEvaluationTestObserver(Arc<dyn Fn(&CompilationFactKey) + Send + Sync>);

#[cfg(test)]
impl FactEvaluationTestObserver {
    pub(crate) fn new(observe: impl Fn(&CompilationFactKey) + Send + Sync + 'static) -> Self {
        Self(Arc::new(observe))
    }

    fn observe(&self, key: &CompilationFactKey) {
        self.0(key);
    }
}

#[cfg(test)]
impl fmt::Debug for FactEvaluationTestObserver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("FactEvaluationTestObserver")
    }
}

#[derive(Debug, Default)]
struct RuntimeState {
    owners: HashMap<CompilationFactKey, FactTaskIdentity>,
    waiting: HashMap<FactTaskIdentity, BTreeMap<WaitEdge, usize>>,
    records: BTreeMap<CompilationFactKey, FactDependencyRecord>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct WaitEdge {
    fact: CompilationFactKey,
    owner: FactTaskIdentity,
}

impl FactRuntime {
    #[cfg(test)]
    pub(crate) fn new(worker_budget: WorkerBudget) -> Self {
        Self::with_profile(worker_budget, None)
    }

    pub(crate) fn with_profile(
        worker_budget: WorkerBudget,
        profile: Option<(CompilationProfileConfiguration, CompilationProfileContext)>,
    ) -> Self {
        let profile = profile.map(|(configuration, context)| {
            ProfileSession::new(configuration, worker_budget.get(), context)
        });

        Self {
            next_task: AtomicU64::new(0),
            state: Mutex::new(RuntimeState::default()),
            inputs: CompilationInputs::default(),
            scheduler: FactScheduler::with_profile(worker_budget, profile.as_ref().map(Arc::clone)),
            profile,
            #[cfg(test)]
            observer: Mutex::new(None),
        }
    }

    pub(crate) fn updated(
        &self,
        worker_budget: WorkerBudget,
        inputs: CompilationInputs,
        profile: Option<Arc<ProfileSession>>,
    ) -> (Self, BTreeSet<CompilationFactKey>) {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|_| panic!("fact dependency state must remain available"));

        let identity_namespace_matches = self.inputs.has_same_identity_namespace(&inputs);

        let (records, invalidated) = retained_records(&state, &inputs, identity_namespace_matches);

        let reusable = records.keys().cloned().collect();

        record_snapshot_profile(profile.as_deref(), &state, &reusable, &invalidated);

        let runtime = Self {
            next_task: AtomicU64::new(0),
            state: Mutex::new(RuntimeState {
                records,
                ..RuntimeState::default()
            }),
            inputs,
            scheduler: FactScheduler::with_profile(worker_budget, profile.as_ref().map(Arc::clone)),
            profile,
            #[cfg(test)]
            observer: Mutex::new(None),
        };

        (runtime, reusable)
    }

    pub(crate) fn set_inputs(&mut self, inputs: CompilationInputs) {
        self.inputs = inputs;
    }

    pub(crate) fn input_snapshot(&self) -> CompilationInputs {
        self.inputs.clone()
    }

    pub(crate) fn record_input(&self, key: CompilationInputKey) -> Result<(), FactQueryError> {
        let fingerprint = key
            .fixed_bit()
            .is_none()
            .then(|| self.inputs.get(&key))
            .flatten();

        record_input(self.identity(), &key, fingerprint)
    }

    pub(crate) fn record_frozen_fact(
        &self,
        key: &CompilationFactKey,
    ) -> Result<(), FactQueryError> {
        let Some(bit) = key.frozen_bit() else {
            return Err(FactRuntimeFailure::InvalidFrozenFact { fact: key.clone() }.into());
        };

        record_frozen_fact(self.identity(), bit)
    }

    #[inline(always)]
    pub(crate) fn profile(&self) -> Option<&ProfileSession> {
        self.profile.as_deref()
    }

    pub(crate) fn profile_session(&self) -> Option<Arc<ProfileSession>> {
        self.profile.as_ref().map(Arc::clone)
    }

    pub(crate) fn profile_configuration(&self) -> Option<CompilationProfileConfiguration> {
        self.profile.as_ref().map(|profile| profile.configuration())
    }

    pub(crate) fn profile_report(&self) -> Option<CompilationProfileReport> {
        self.profile.as_ref().map(|profile| profile.report())
    }

    pub(crate) fn run<T>(
        &self,
        priority: QueryPriority,
        operation: impl FnOnce() -> Result<T, FactQueryError> + Send,
    ) -> Result<T, FactQueryError>
    where
        T: Send,
    {
        self.scheduler.run(priority, operation)?
    }

    pub(crate) fn run_demand<T>(
        &self,
        priority: &QueryPriorityDemand,
        operation: impl FnOnce() -> Result<T, FactQueryError> + Send,
    ) -> Result<T, FactQueryError>
    where
        T: Send,
    {
        self.scheduler.run_demand(priority, operation)?
    }

    pub(crate) fn current_priority(&self) -> Result<Option<QueryPriority>, FactQueryError> {
        self.scheduler.current_priority()
    }

    pub(crate) fn promote_priority(
        &self,
        priority: &QueryPriorityDemand,
        requested: QueryPriority,
    ) {
        self.scheduler.promote(priority, requested);
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

        let priority = self.current_priority()?.unwrap_or(QueryPriority::Normal);

        let results = self.scheduler.map_indexed(priority, len, |index| {
            run_with_evaluations(&evaluations, || Ok(operation(index)))
        })?;

        results.into_iter().collect()
    }

    pub(crate) fn check_request_cycle(
        &self,
        cycle_key: &CompilationFactKey,
    ) -> Result<(), FactQueryError> {
        check_request_cycle(self.identity(), cycle_key)
    }

    pub(crate) fn record_completed_request(
        &self,
        key: &CompilationFactKey,
    ) -> Result<(), FactQueryError> {
        record_completed_request(self.identity(), key)
    }

    pub(crate) fn current_task_context(&self) -> Result<Option<FactTaskContext>, FactQueryError> {
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
            .map_err(|_| FactRuntimeFailure::CapacityExhausted {
                resource: CapacityResource::TaskIdentity,
                fact: Some(key.clone()),
                task: None,
            })?;

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

        let mut state = self.state_for(Some(&key), Some(task))?;

        match state.owners.entry(key.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(task);
            }
            Entry::Occupied(entry) => {
                return Err(FactRuntimeFailure::PublicationMismatch {
                    requested: PublicationIdentity {
                        task: Some(task),
                        fact: key.clone(),
                    },
                    actual: PublicationState::Computing {
                        task: *entry.get(),
                        fact: key,
                    },
                }
                .into());
            }
        }

        let evaluation = EvaluationGuard {
            runtime: self,
            key,
            context,
            active: true,
        };

        drop(state);

        #[cfg(test)]
        self.observe(&evaluation.key)?;

        Ok(evaluation)
    }

    #[cfg(test)]
    pub(crate) fn set_test_observer(
        &self,
        observer: FactEvaluationTestObserver,
    ) -> Result<(), FactQueryError> {
        let mut current =
            self.observer
                .lock()
                .map_err(|_| FactRuntimeFailure::SynchronizationPoisoned {
                    component: SynchronizationComponent::RuntimeDependencies,
                    fact: None,
                    task: None,
                })?;

        *current = Some(observer);

        Ok(())
    }

    #[cfg(test)]
    fn observe(&self, key: &CompilationFactKey) -> Result<(), FactQueryError> {
        let observer = self
            .observer
            .lock()
            .map_err(|_| FactRuntimeFailure::SynchronizationPoisoned {
                component: SynchronizationComponent::RuntimeDependencies,
                fact: Some(key.clone()),
                task: None,
            })?
            .clone();

        if let Some(observer) = observer {
            observer.observe(key);
        }

        Ok(())
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
        let requester = self.current_task_context()?;
        let requester_identity = requester.as_ref().map(FactTaskContext::identity);
        let mut state = self.state_for(Some(key), requester_identity)?;

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
            .ok_or_else(|| FactRuntimeFailure::CapacityExhausted {
                resource: CapacityResource::SchedulerWaitRegistrations,
                fact: Some(key.clone()),
                task: Some(requester_identity),
            })?;

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

    fn state_for(
        &self,
        fact: Option<&CompilationFactKey>,
        task: Option<FactTaskIdentity>,
    ) -> Result<MutexGuard<'_, RuntimeState>, FactQueryError> {
        self.state.lock().map_err(|_| {
            FactRuntimeFailure::SynchronizationPoisoned {
                component: SynchronizationComponent::RuntimeDependencies,
                fact: fact.cloned(),
                task,
            }
            .into()
        })
    }

    fn identity(&self) -> RuntimeIdentity {
        RuntimeIdentity(std::ptr::from_ref(self).addr())
    }

    fn prepare_evaluation<'runtime, T>(
        &'runtime self,
        key: &CompilationFactKey,
        context: &FactTaskContext,
        value: &T,
    ) -> Result<EvaluationCommit<'runtime>, FactQueryError>
    where
        T: Hash + ?Sized,
    {
        let state = self.state_for(Some(key), Some(context.identity()))?;

        if state.owners.get(key).copied() != Some(context.identity()) {
            let actual = state
                .owners
                .get(key)
                .copied()
                .map_or(PublicationState::Vacant, |task| {
                    PublicationState::Computing {
                        task,
                        fact: key.clone(),
                    }
                });

            return Err(FactRuntimeFailure::PublicationMismatch {
                requested: PublicationIdentity {
                    task: Some(context.identity()),
                    fact: key.clone(),
                },
                actual,
            }
            .into());
        }

        let dependencies = context.finish()?;

        let mut input_dependencies = dependencies.inputs;

        for (index, input) in CompilationInputKey::FIXED.iter().enumerate() {
            if dependencies.fixed_inputs & (1 << index) == 0 {
                continue;
            }

            let Some(fingerprint) = self.inputs.get(input) else {
                return Err(FactRuntimeFailure::MissingInputFingerprint {
                    input: input.clone(),
                    task: context.identity(),
                    fact: key.clone(),
                }
                .into());
            };

            input_dependencies.insert(input.clone(), fingerprint);
        }

        let mut fact_dependencies = dependencies.facts;

        for (index, fact) in CompilationFactKey::FROZEN.iter().enumerate() {
            if dependencies.frozen_facts & (1 << index) != 0 {
                fact_dependencies.insert(fact.clone());
            }
        }

        let facts = fact_dependencies
            .iter()
            .map(|dependency| {
                state
                    .records
                    .get(dependency)
                    .map(FactDependencyRecord::fingerprint)
                    .map(|fingerprint| (dependency.clone(), fingerprint))
                    .ok_or_else(|| {
                        FactQueryError::from(FactRuntimeFailure::MissingDependencyRecord {
                            task: context.identity(),
                            fact: key.clone(),
                            dependency: dependency.clone(),
                        })
                    })
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        let record =
            FactDependencyRecord::new(fact_fingerprint(key, value), facts, input_dependencies);

        // The commit owns the key while coordinating runtime and cache publication locks.
        Ok(EvaluationCommit {
            state,
            task: context.identity(),
            key: Some(key.clone()),
            record: Some(record),
        })
    }

    fn abandon_evaluation(&self, context: &FactTaskContext, key: &CompilationFactKey) {
        // Guard cleanup is best effort because Drop cannot return a poisoned runtime error.
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        remove_evaluation(&mut state, context.identity(), key);
    }

    fn finish_waiting(&self, task: FactTaskIdentity, edge: &WaitEdge) {
        // Wait-guard cleanup is best effort because Drop cannot return a poisoned runtime error.
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
        let state = self.state_for(Some(key), None)?;

        Ok(state
            .records
            .get(key)
            .map(|record| record.facts().keys().cloned().collect::<Box<[_]>>()))
    }

    #[cfg(test)]
    pub(crate) fn input_dependencies(
        &self,
        key: &CompilationFactKey,
    ) -> Result<Option<Box<[CompilationInputKey]>>, FactQueryError> {
        let state = self.state_for(Some(key), None)?;

        Ok(state
            .records
            .get(key)
            .map(|record| record.inputs().keys().cloned().collect::<Box<[_]>>()))
    }
}

fn retained_records(
    state: &RuntimeState,
    inputs: &CompilationInputs,
    identity_namespace_matches: bool,
) -> (
    BTreeMap<CompilationFactKey, FactDependencyRecord>,
    BTreeSet<CompilationFactKey>,
) {
    let mut invalidated = direct_invalidations(state, inputs, identity_namespace_matches);
    let dependents = reverse_dependencies(state);
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

    let retained = state
        .records
        .iter()
        .filter(|(fact, _)| !invalidated.contains(*fact))
        .map(|(fact, record)| (fact.clone(), record.clone()))
        .collect();

    (retained, invalidated)
}

fn direct_invalidations(
    state: &RuntimeState,
    inputs: &CompilationInputs,
    identity_namespace_matches: bool,
) -> BTreeSet<CompilationFactKey> {
    state
        .records
        .iter()
        .filter(|(key, record)| {
            (!identity_namespace_matches && !key.has_stable_snapshot_identity())
                || record
                    .inputs()
                    .iter()
                    .any(|(key, fingerprint)| inputs.get(key) != Some(*fingerprint))
                || record.facts().iter().any(|(key, fingerprint)| {
                    state
                        .records
                        .get(key)
                        .map(FactDependencyRecord::fingerprint)
                        != Some(*fingerprint)
                })
        })
        .map(|(key, _)| key.clone())
        .collect()
}

fn reverse_dependencies(
    state: &RuntimeState,
) -> BTreeMap<CompilationFactKey, Vec<CompilationFactKey>> {
    let mut dependents = BTreeMap::<CompilationFactKey, Vec<CompilationFactKey>>::new();

    for (fact, record) in &state.records {
        for dependency in record.facts().keys() {
            dependents
                .entry(dependency.clone())
                .or_default()
                .push(fact.clone());
        }
    }

    dependents
}

fn record_snapshot_profile(
    profile: Option<&ProfileSession>,
    state: &RuntimeState,
    reusable: &BTreeSet<CompilationFactKey>,
    invalidated: &BTreeSet<CompilationFactKey>,
) {
    let Some(profile) = profile else {
        return;
    };

    for fact in reusable {
        profile.record_cross_snapshot_reuse(ProfileQueryKind::from_key(fact));
    }

    for fact in invalidated
        .iter()
        .filter(|fact| state.records.contains_key(*fact))
    {
        profile.record_invalidation(ProfileQueryKind::from_key(fact));
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
                        return Err(FactRuntimeFailure::InvalidWaitGraph {
                            requester,
                            owner,
                            missing_predecessor: cursor,
                            requested: requested_key.clone(),
                        }
                        .into());
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

    pub(crate) fn prepare<T>(mut self, value: &T) -> Result<EvaluationCommit<'a>, FactQueryError>
    where
        T: Hash + ?Sized,
    {
        let commit = self
            .runtime
            .prepare_evaluation(&self.key, &self.context, value)?;

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
    record: Option<FactDependencyRecord>,
}

impl EvaluationCommit<'_> {
    pub(crate) fn commit(mut self) {
        let Some(key) = self.key.take() else {
            return;
        };

        remove_evaluation(&mut self.state, self.task, &key);

        let Some(record) = self.record.take() else {
            return;
        };

        self.state.records.insert(key, record);
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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::{Arc, Mutex};

    use bray_declarations::ModulePartId;
    use bray_source::SourceId;

    use super::{FactEvaluationTestObserver, FactRuntime};
    use crate::WorkerBudget;
    use crate::fact::{
        CancellationToken, CompilationFactKey, CompilationInputKey, CompilationInputs, FactCell,
        FactDependencyRecord, FactQueryError, FactRuntimeFailure, SynchronizationComponent,
        fact_fingerprint,
    };

    #[test]
    fn evaluation_observation_records_only_started_computations() {
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();
        let cell = FactCell::new();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observer_keys = Arc::clone(&observed);

        runtime
            .set_test_observer(FactEvaluationTestObserver::new(move |key| {
                observer_keys
                    .lock()
                    .unwrap_or_else(|_| panic!("evaluation log must remain available"))
                    .push(key.clone());
            }))
            .unwrap_or_else(|error| panic!("runtime must accept test observation: {error:?}"));

        let first = cell.get_or_compute(
            &runtime,
            CompilationFactKey::SyntaxTree,
            &cancellation,
            || Ok(7_u32),
        );

        let repeated = cell.get_or_compute(
            &runtime,
            CompilationFactKey::SyntaxTree,
            &cancellation,
            || Ok(9_u32),
        );

        assert_eq!(first, Ok(&7));
        assert_eq!(repeated, Ok(&7));

        assert_eq!(
            *observed
                .lock()
                .unwrap_or_else(|_| panic!("evaluation log must remain available")),
            [CompilationFactKey::SyntaxTree]
        );
    }

    #[test]
    fn input_fingerprints_drive_exact_transitive_reuse() {
        let mut runtime = FactRuntime::default();
        let input_key = CompilationInputKey::SourceDiagnostics;
        let mut inputs = CompilationInputs::default();

        inputs.insert(input_key.clone(), &7_u8);
        runtime.set_inputs(inputs.clone());

        let cancellation = CancellationToken::new();
        let source = FactCell::new();
        let consumer = FactCell::new();
        let source_key = CompilationFactKey::SyntaxTree;
        let consumer_key = CompilationFactKey::DeclarationTable;

        let result = consumer.get_or_compute(&runtime, consumer_key.clone(), &cancellation, || {
            source.get_or_compute(&runtime, source_key.clone(), &cancellation, || {
                runtime.record_input(input_key.clone())?;

                Ok(11_u8)
            })?;

            Ok(13_u8)
        });

        assert_eq!(result, Ok(&13));

        let (_, unchanged) = runtime.updated(WorkerBudget::serial(), inputs, None);

        assert_eq!(
            unchanged,
            BTreeSet::from([source_key.clone(), consumer_key.clone()])
        );

        let mut changed_inputs = CompilationInputs::default();
        changed_inputs.insert(input_key, &8_u8);

        let (_, changed) = runtime.updated(WorkerBudget::serial(), changed_inputs, None);

        assert!(changed.is_empty());
    }

    #[test]
    fn snapshot_local_semantic_keys_are_not_compared_across_snapshots() {
        let mut runtime = FactRuntime::default();
        let key = CompilationFactKey::ModuleContributionGate(ModulePartId::new(7));
        let mut inputs = CompilationInputs::default();

        inputs.insert(CompilationInputKey::SourceSet, &[1_u8]);
        runtime.set_inputs(inputs);

        runtime
            .state_for(None, None)
            .unwrap_or_else(|error| panic!("runtime state must be available: {error:?}"))
            .records
            .insert(
                key.clone(),
                FactDependencyRecord::new(
                    fact_fingerprint(&key, &17_u8),
                    BTreeMap::new(),
                    BTreeMap::new(),
                ),
            );

        let mut updated_inputs = CompilationInputs::default();

        updated_inputs.insert(CompilationInputKey::SourceSet, &[2_u8]);

        let (_, reusable) = runtime.updated(WorkerBudget::serial(), updated_inputs, None);

        assert!(reusable.is_empty());
    }

    #[test]
    fn deep_and_expansion_heavy_invalidation_is_iterative_and_precise() {
        const DEPTH: u32 = 20_000;
        const WIDTH: u32 = 20_000;

        let mut runtime = FactRuntime::default();
        let deep_root = source_syntax_key(0);
        let wide_root = CompilationFactKey::SyntaxTree;
        let unrelated = CompilationFactKey::CheckDiagnostics;

        let mut previous_inputs = CompilationInputs::default();
        previous_inputs.insert(CompilationInputKey::SourceDiagnostics, &0_u8);
        previous_inputs.insert(CompilationInputKey::ProductKind, &0_u8);
        runtime.set_inputs(previous_inputs.clone());

        {
            let mut state = runtime
                .state_for(None, None)
                .unwrap_or_else(|error| panic!("runtime state must be available: {error:?}"));

            let deep_input = previous_inputs
                .get(&CompilationInputKey::SourceDiagnostics)
                .unwrap_or_else(|| panic!("deep invalidation input must exist"));

            state.records.insert(
                deep_root.clone(),
                FactDependencyRecord::new(
                    fact_fingerprint(&deep_root, &deep_root),
                    BTreeMap::new(),
                    BTreeMap::from([(CompilationInputKey::SourceDiagnostics, deep_input)]),
                ),
            );

            for index in 1..DEPTH {
                let key = source_syntax_key(index);
                let dependency = source_syntax_key(index - 1);

                let fingerprint = state
                    .records
                    .get(&dependency)
                    .map(FactDependencyRecord::fingerprint)
                    .unwrap_or_else(|| panic!("deep dependency record must exist"));

                state.records.insert(
                    key.clone(),
                    FactDependencyRecord::new(
                        fact_fingerprint(&key, &key),
                        BTreeMap::from([(dependency, fingerprint)]),
                        BTreeMap::new(),
                    ),
                );
            }

            let wide_input = previous_inputs
                .get(&CompilationInputKey::ProductKind)
                .unwrap_or_else(|| panic!("wide invalidation input must exist"));

            state.records.insert(
                wide_root.clone(),
                FactDependencyRecord::new(
                    fact_fingerprint(&wide_root, &wide_root),
                    BTreeMap::new(),
                    BTreeMap::from([(CompilationInputKey::ProductKind, wide_input)]),
                ),
            );

            for index in 0..WIDTH {
                let key = declaration_chunk_key(index);

                let fingerprint = state
                    .records
                    .get(&wide_root)
                    .map(FactDependencyRecord::fingerprint)
                    .unwrap_or_else(|| panic!("wide dependency record must exist"));

                state.records.insert(
                    key.clone(),
                    FactDependencyRecord::new(
                        fact_fingerprint(&key, &key),
                        BTreeMap::from([(wide_root.clone(), fingerprint)]),
                        BTreeMap::new(),
                    ),
                );
            }

            state.records.insert(
                unrelated.clone(),
                FactDependencyRecord::new(
                    fact_fingerprint(&unrelated, &unrelated),
                    BTreeMap::new(),
                    BTreeMap::new(),
                ),
            );
        }

        let mut updated_inputs = CompilationInputs::default();
        updated_inputs.insert(CompilationInputKey::SourceDiagnostics, &1_u8);
        updated_inputs.insert(CompilationInputKey::ProductKind, &1_u8);

        let (_, reusable) = runtime.updated(WorkerBudget::serial(), updated_inputs, None);

        assert_eq!(reusable, BTreeSet::from([unrelated]));
        assert!(!reusable.contains(&deep_root));
        assert!(!reusable.contains(&wide_root));
        assert!(!reusable.contains(&source_syntax_key(DEPTH - 1)));
        assert!(!reusable.contains(&declaration_chunk_key(WIDTH - 1)));
    }

    #[test]
    fn non_frozen_facts_retain_the_rejected_fact_identity() {
        let runtime = FactRuntime::default();

        let error = match runtime.record_frozen_fact(&CompilationFactKey::DeclarationTable) {
            Ok(()) => panic!("a non-frozen fact must not be recorded as frozen"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            FactQueryError::Runtime(error)
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::InvalidFrozenFact {
                        fact: CompilationFactKey::DeclarationTable,
                    }
                )
        ));
    }

    #[test]
    fn poisoned_dependency_state_reports_the_runtime_component() {
        let runtime = FactRuntime::default();

        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _state = runtime
                .state
                .lock()
                .unwrap_or_else(|_| panic!("test runtime state should begin available"));

            panic!("poison runtime dependency state");
        }));

        let error = match runtime.dependencies(&CompilationFactKey::SyntaxTree) {
            Ok(_) => panic!("dependency lookup must report poisoned runtime state"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            FactQueryError::Runtime(error)
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::SynchronizationPoisoned {
                        component: SynchronizationComponent::RuntimeDependencies,
                        fact: Some(CompilationFactKey::SyntaxTree),
                        task: None,
                    }
                )
        ));
    }

    #[test]
    fn poisoned_evaluation_state_retains_fact_and_task_identity() {
        let runtime = FactRuntime::default();
        let fact = CompilationFactKey::CheckDiagnostics;

        let context = runtime
            .task_with_cycle_key(fact.clone(), fact.clone())
            .unwrap_or_else(|error| panic!("test task must be available: {error:?}"));

        let task = context.identity();

        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _state = runtime
                .state
                .lock()
                .unwrap_or_else(|_| panic!("test runtime state should begin available"));

            panic!("poison runtime evaluation state");
        }));

        let error = match runtime.begin(context) {
            Ok(_) => panic!("evaluation must report poisoned runtime state"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            FactQueryError::Runtime(error)
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::SynchronizationPoisoned {
                        component: SynchronizationComponent::RuntimeDependencies,
                        fact: Some(actual_fact),
                        task: Some(actual_task),
                    } if actual_fact == &fact && *actual_task == task
                )
        ));
    }

    fn source_syntax_key(index: u32) -> CompilationFactKey {
        CompilationFactKey::SourceUnitSyntax(SourceId::new(index))
    }

    fn declaration_chunk_key(index: u32) -> CompilationFactKey {
        CompilationFactKey::DeclarationChunk(SourceId::new(index))
    }
}
