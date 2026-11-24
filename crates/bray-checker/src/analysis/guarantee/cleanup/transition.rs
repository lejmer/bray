use std::sync::Arc;

use bray_bound_tree::{
    AnyBoundNodeId, AsyncStorageExitDecision, AsyncStorageExitDisposition,
    AsyncStorageExitRecoveryCause, StorageAccessId, StorageCleanupAction, StorageCleanupEntry,
    StorageExitPoint, StorageMutation,
};
use bray_symbols::TypeAssociatedLifecycleSlot;

use super::super::super::model::{AnalysisOperation, AnalysisOperationKind, AnalysisScopeExitPhase};
use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use super::plan::Lifecycle;
use crate::{CheckerRequestContext, ExecutionCallEvidence, ExecutionPlace};

pub(in crate::analysis) type Observer<'a> = dyn FnMut(CleanupObservation<'_>) + 'a;

pub(in crate::analysis) enum CleanupObservation<'a> {
    Cancellation,
    Continuation(Box<ExecutionState>),
    Part {
        access: StorageAccessId,
        ty: bray_symbols::TypeId,
        releases_storage: bool,
        before: &'a ExecutionState,
    },
    Invocation {
        access: StorageAccessId,
        part: usize,
        slot: TypeAssociatedLifecycleSlot,
        lifecycle: &'a Lifecycle,
        whole: bool,
        represented: bool,
        evidence: &'a Arc<ExecutionCallEvidence>,
        proof: &'a Result<(), crate::UnprovenExecutionCondition>,
    },
}

pub(in crate::analysis) struct LifecycleInvocation<'a> {
    pub(in crate::analysis) ty: bray_symbols::TypeId,
    pub(in crate::analysis) access: StorageAccessId,
    pub(in crate::analysis) lifecycle: &'a Lifecycle,
    pub(in crate::analysis) entry: &'a ExecutionCallEvidence,
    pub(in crate::analysis) contract: &'a bray_symbols::DependencyContractTemplateData,
    pub(in crate::analysis) completes: bool,
    pub(in crate::analysis) path: Option<&'a bray_bound_tree::StoragePath>,
    pub(in crate::analysis) route: Option<&'a bray_bound_tree::StorageReferencePath>,
    pub(in crate::analysis) outcomes: Vec<(usize, &'a bray_symbols::CapturedValueOutcome, crate::ExecutionCondition)>,
}

impl<'a> LifecycleInvocation<'a> {
    pub(in crate::analysis) fn normal_outcomes(&self, selected: Option<usize>) -> impl Iterator<Item = &'a bray_symbols::CapturedValueOutcome> + Clone + '_ {
        self.outcomes.iter().filter_map(move |(index, row, _)| selected.is_none_or(|selected| *index == selected).then_some(*row))
    }
}

pub(in crate::analysis) enum CleanupMutation<'a> {
    /// The owning analysis proved that cleanup leaves input observations unchanged.
    Unchanged,
    /// Source cleanup ordering was diagnosed and has no valid selected order.
    Recovery,
    /// Revisits the same abstract array region using its reached state.
    Repeat(bray_bound_tree::StorageCleanupRemainder),
    Opaque {
        ty: bray_symbols::TypeId,
        access: StorageAccessId,
        path: Option<&'a bray_bound_tree::StoragePath>,
        route: Option<&'a bray_bound_tree::StorageReferencePath>,
        failures: &'static [bray_symbols::DependencyFailureKind],
    },
    Lifecycle(LifecycleInvocation<'a>),
}

pub(in crate::analysis) enum CleanupTransition<'a> {
    PartAbsent {
        access: StorageAccessId, part: usize, phase: bray_bound_tree::AsyncCleanupPhases,
        owner: Option<&'a bray_bound_tree::StoragePath>,
    },
    BeginConditional(Vec<bray_bound_tree::StoragePath>),
    Action(&'a mut ExecutionState, StorageMutation, CleanupMutation<'a>),
    EndConditional { completes: bool },
}

pub(in crate::analysis) type Mutation<'a> =
    dyn FnMut(CleanupTransition<'_>) -> bool + 'a;

pub(in crate::analysis) struct CleanupSelection {
    pub(in crate::analysis) accesses: Option<Arc<[StorageAccessId]>>,
    pub(in crate::analysis) recoveries: Vec<AsyncStorageExitDecision>,
    pub(in crate::analysis) after: Option<StorageCleanupAction>,
    pub(in crate::analysis) replacement: Option<bray_bound_tree::StorageTarget>,
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn cleanup_receiver_writes<'rows>(
        &self,
        contract: &'rows bray_symbols::DependencyContractTemplateData,
        entry: &ExecutionCallEvidence,
        mutation: StorageMutation,
    ) -> Vec<(&'rows bray_symbols::DependencySubject, &'rows bray_symbols::CapturedValueTemplate)> {
        use bray_symbols::DependencySubjectRoot;

        match mutation {
            StorageMutation::CleanupOutcome { row, .. } =>
                contract.captured_values()[row].bindings.iter()
                    .filter(|(subject, _)| subject.subject_root() == DependencySubjectRoot::Receiver)
                    .map(|(subject, value)| (subject, value)).collect(),
            StorageMutation::CleanupFailure { kind, row, .. } => {
                let mut candidates = contract.failure_outcomes().into_iter().flatten().enumerate().filter(|(index, outcome)| {
                    outcome.covers(kind) && row.map_or_else(|| entry.admits_failure(*index), |row| row == *index)
                }).map(|(_, outcome)| &outcome.writes);

                let Some(first) = candidates.next() else { return Vec::new(); };

                first.iter().filter(|(subject, value)| {
                    subject.subject_root() == DependencySubjectRoot::Receiver && candidates.clone().all(|candidate| {
                        candidate.binary_search_by(|(other, _)| other.cmp(subject))
                            .is_ok_and(|index| candidate[index].1 == *value)
                    })
                }).map(|(subject, value)| (subject, value)).collect()
            }
            _ => Vec::new(),
        }
    }

    pub(in crate::analysis) fn cleanup_normal_guard(
        &self,
        state: &ExecutionState,
        entry: &ExecutionCallEvidence,
        row: &bray_symbols::CapturedValueOutcome,
    ) -> Option<crate::ExecutionCondition> {
        let guard = row.guard.map_or_else(|| crate::ExecutionCondition::boolean(true), |guard| {
            let result = row.result_variant().map_or_else(crate::ExecutionCondition::unknown, |tag| {
                crate::ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(tag, Arc::new(crate::ExecutionCondition::unknown())))
            });

            entry.bind_term(self.request.semantic_values(), guard, &result)
        });

        state.ordinary_proof().can_assume(&guard, true).then_some(guard)
    }

    pub(in crate::analysis) fn assume_cleanup_guard(
        &self,
        state: &mut ExecutionState,
        guard: crate::ExecutionCondition,
    ) {
        guard.assume(true, &mut state.assumptions);
        state.refine_witnesses();
    }

    pub(in crate::analysis) fn assume_cleanup_failure(
        &self,
        state: &mut ExecutionState,
        entry: &ExecutionCallEvidence,
        row: Option<usize>,
    ) -> bool {
        if row.is_some_and(|row| !entry.admits_failure(row)) {
            return false;
        }

        let Some(guard) = row.and_then(|row| entry.failure_guard(row)) else {
            return true;
        };

        if !state.ordinary_proof().can_assume(guard, true) {
            return false;
        }

        self.assume_cleanup_guard(state, guard.clone());

        true
    }

    pub(in crate::analysis::guarantee) fn transition_cleanup(
        &self,
        state: &mut ExecutionState,
        point: &AnalysisOperation,
        mut observe: Option<&mut Observer<'_>>,
    ) -> bool {
        let operation = point.kind();
        let flow = self.checked_body.expect("cleanup replay retains its reached storage flow").storage_flow();

        match operation {
            AnalysisOperationKind::ScopeExit {
                scope, exit, phase, ..
            } => {
                let exit_point = StorageExitPoint::new(scope, exit);

                if phase == AnalysisScopeExitPhase::LifecycleResolution
                    && let Some(remainder) = crate::analysis::outcomes::CleanupProgress::resume(&mut state.cleanup, Some(exit_point), point.id())
                {
                    let Some(entry) = flow.cleanup_entry(point.id(), Some(remainder.after)) else { return false; };

                    return self.execute_scope_cleanup(state, point, Some(&remainder.accesses),
                        Some(remainder.after), Some(entry), &mut observe, &mut None);
                }

                let Some(entry) = flow.cleanup_entry(point.id(), None) else { return false; };

                let order_failure = StorageMutation::ScopeCleanupOrder {
                    operation: point.id(),
                    point: exit_point,
                };

                if flow.outcome_mutation(order_failure).is_some() {
                    return self.execute_scope_cleanup(state, point, None,
                        None, None, &mut observe, &mut None);
                }

                self.replay_cleanup_orders(state, point, entry, &mut observe)
            }
            AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(_))
            | AnalysisOperationKind::AssignmentInstall(_) => {
                let remainder = crate::analysis::outcomes::CleanupProgress::resume(&mut state.cleanup, None, point.id());
                let after = remainder.as_ref().map(|remainder| remainder.after);

                let Some(entry) = flow.cleanup_entry(point.id(), after) else { return remainder.is_none(); };

                let remaining = if let Some(remainder) = &remainder {
                    remainder.accesses.as_ref()
                } else {
                    let [order] = entry.orders() else {
                        assert!(entry.orders().is_empty(), "replacement cleanup has one displaced owner");

                        return true;
                    };

                    order.as_ref()
                };

                self.execute_replacement_cleanup(
                    state, remaining, Some(entry), &mut observe, point, after, entry.replacement(), &mut None,
                )
            }
            // These operations do not dispose storage. Their explicit invocation
            // effects are handled by the ordinary operation transition.
            AnalysisOperationKind::Result { .. }
            | AnalysisOperationKind::ScopedResult(_)
            | AnalysisOperationKind::Bound(_)
            | AnalysisOperationKind::PatternObservation(_)
            | AnalysisOperationKind::Call { .. }
            | AnalysisOperationKind::Suspension { .. }
            | AnalysisOperationKind::Recovery(_) => true,
        }
    }

    fn replay_cleanup_orders(
        &self,
        state: &mut ExecutionState,
        point: &AnalysisOperation,
        entry: &StorageCleanupEntry,
        observe: &mut Option<&mut Observer<'_>>,
    ) -> bool {
        let order = match entry.orders() {
            [] => &[][..],
            [order] => order.as_ref(),
            _ => return self.replay_distinct_cleanup_orders(state, point, entry, observe),
        };

        self.execute_scope_cleanup(state, point, Some(order), None, Some(entry), observe, &mut None)
    }

    fn replay_distinct_cleanup_orders(
        &self,
        state: &mut ExecutionState,
        point: &AnalysisOperation,
        entry: &StorageCleanupEntry,
        observe: &mut Option<&mut Observer<'_>>,
    ) -> bool {
        let required = entry.orders().iter().flat_map(|order| order.iter().copied()).collect::<std::collections::BTreeSet<_>>()
            .into_iter().filter(|access| self.disposal.steps[access].iter().enumerate().any(|(part, step)| {
                step.phases().includes_lifecycle()
                    && self.cleanup_part_condition(state, step.selection(), |_| {
                        entry.part_absent(*access, part, bray_bound_tree::AsyncCleanupPhases::Lifecycle)
                    }, None)
                        .active
                        .prove_ordinary(&state.assumptions, Some(&state.proofs)) == Some(true)
            })).collect::<Vec<_>>();

        let mut viable = entry.orders().iter().filter(|order| required.iter().all(|access| order.contains(access))).peekable();

        let Some(first) = viable.next() else { return false; };

        if viable.peek().is_none() {
            return self.execute_scope_cleanup(state, point, Some(first), None, Some(entry), observe, &mut None);
        }

        let mut completed = None;

        for order in std::iter::once(first).chain(viable) {
            let mut branch = state.clone();

            if self.execute_scope_cleanup(&mut branch, point, Some(order), None, Some(entry), observe, &mut None) {
                super::super::flow::merge_execution(&mut completed, &Some(branch), super::super::flow::ExecutionJoin::Boundary);
            }
        }

        let Some(completed) = completed else { return false; };

        *state = completed;

        true
    }

    pub(in crate::analysis) fn replacement_cleanup_required(&self, access: StorageAccessId) -> bool {
        matches!(self.disposal.requirements.replacement(access), bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(_))
    }

    pub(in crate::analysis) fn select_scope_cleanup(
        &self,
        state: &crate::analysis::storage_flow::StorageFlowState,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        scope: bray_bound_tree::StorageScope,
        phase: AnalysisScopeExitPhase,
    ) -> CleanupSelection {
        let mut accesses = Vec::new();
        let mut recoveries = Vec::new();

        let requirements = &self.disposal.requirements;

        for identity in requirements.scope_candidates(scope).rev().filter(|identity| state.live.contains(identity)) {
            let requirement = requirements.get(identity).expect("scope candidate has a checked requirement");
            let moved = crate::storage::paths_at_root(&state.moved, Some(identity)).map(|(path, _)| path);

            let disposition = requirement.exit_disposition(
                self.storage,
                scope,
                state.fully_moved.contains(&identity),
                moved,
                state.absent_paths(Some(identity)),
            );

            match disposition {
                AsyncStorageExitDisposition::Recovered(_) => {
                    recoveries.push(AsyncStorageExitDecision::new(identity, disposition));
                }
                AsyncStorageExitDisposition::Cleanup { access, phases, .. } => {
                    let included = match phase {
                        AnalysisScopeExitPhase::TaskCancellationBroadcast => phases.includes_cancellation(),
                        AnalysisScopeExitPhase::LifecycleResolution => phases.includes_lifecycle(),
                    };

                    if included {
                        accesses.push(access);
                    }
                }
                AsyncStorageExitDisposition::NoCleanup => {}
            }
        }

        let schedule = match phase {
            AnalysisScopeExitPhase::TaskCancellationBroadcast => Ok(accesses),
            AnalysisScopeExitPhase::LifecycleResolution => dependencies.lifecycle_order(self.storage, accesses),
        };

        let accesses = match schedule {
            Ok(order) => Some(order.into()),
            Err(access) => {
                let identity = self.storage.root_identity(access).expect("lifecycle cleanup has a checked root");

                recoveries.push(AsyncStorageExitDecision::new(
                    identity,
                    AsyncStorageExitDisposition::Recovered(AsyncStorageExitRecoveryCause::UnavailableCleanupOrder),
                ));

                None
            }
        };

        CleanupSelection { accesses, recoveries, after: None, replacement: None }
    }

    pub(in crate::analysis) fn transition_storage_cleanup(
        &self,
        state: &mut ExecutionState,
        point: &AnalysisOperation,
        selected: &CleanupSelection,
        mut observe: Option<&mut Observer<'_>>,
        mutate: &mut Mutation<'_>,
    ) -> bool {
        let schedule = selected.accesses.as_deref();

        if matches!(point.kind(), AnalysisOperationKind::ScopeExit { .. }) {
            self.execute_scope_cleanup(state, point, schedule, selected.after, None, &mut observe, &mut Some(mutate))
        } else {
            self.execute_replacement_cleanup(state, schedule.expect("replacement uses a checked cleanup order"),
                None, &mut observe, point, selected.after, selected.replacement.as_ref(), &mut Some(mutate))
        }
    }

    #[expect(clippy::too_many_arguments, reason = "replacement retains its checked order and reached continuation")]
    fn execute_replacement_cleanup(
        &self,
        state: &mut ExecutionState,
        schedule: &[StorageAccessId],
        entry: Option<&StorageCleanupEntry>,
        observe: &mut Option<&mut Observer<'_>>,
        point: &AnalysisOperation,
        mut after: Option<StorageCleanupAction>,
        replacement: Option<&bray_bound_tree::StorageTarget>,
        mutate: &mut Option<&mut Mutation<'_>>,
    ) -> bool {
        let reached = replacement.filter(|target| target.is_exact())
            .and_then(bray_bound_tree::StorageTarget::value_observation_path);

        let path = |access| self.storage.logical_path(access).map(|source| (source, reached.as_ref()));

        if after.is_none() {
            for &access in schedule {
                self.cancel_cleanup(state, access, entry, observe, point, path(access), mutate);
            }
        }

        for index in 0..schedule.len() {
            if !self.dispose(state, &schedule[index..], entry, observe, point, after.take(), path(schedule[index]), mutate) {
                return false;
            }
        }

        true
    }

    fn execute_scope_cleanup(
        &self,
        state: &mut ExecutionState,
        point: &AnalysisOperation,
        schedule: Option<&[StorageAccessId]>,
        mut after: Option<StorageCleanupAction>,
        entry: Option<&StorageCleanupEntry>,
        observe: &mut Option<&mut Observer<'_>>,
        mutate: &mut Option<&mut Mutation<'_>>,
    ) -> bool {
        let operation = point.kind();

        let AnalysisOperationKind::ScopeExit { scope, exit, phase, .. } = operation else {
            panic!("scheduled scope cleanup requires a scope exit: {operation:?}");
        };

        match schedule {
            Some(accesses) => {
                for (index, &access) in accesses.iter().enumerate() {
                    let previous = after.take();

                    let selected_part = previous.is_some_and(|action| {
                        action.kind == bray_bound_tree::StorageCleanupActionKind::Lifecycle(TypeAssociatedLifecycleSlot::Finalizer)
                    });

                    if !selected_part && ExecutionPlace::storage(self.storage, access)
                        .is_some_and(|place| state.current.is_subtree_absent(&place))
                    {
                        continue;
                    }

                    match phase {
                        AnalysisScopeExitPhase::TaskCancellationBroadcast => {
                            self.cancel_cleanup(state, access, entry, observe, point, None, mutate);
                        }
                        AnalysisScopeExitPhase::LifecycleResolution => {
                            if !self.dispose(state, &accesses[index..], entry, observe, point, previous, None, mutate) {
                                return false;
                            }
                        }
                    }
                }
            }
            None => {
                assert_eq!(phase, AnalysisScopeExitPhase::LifecycleResolution,
                    "cancel broadcast keeps its scheduled accesses: {operation:?}");

                let mutation = StorageMutation::ScopeCleanupOrder {
                    operation: point.id(),
                    point: StorageExitPoint::new(scope, exit),
                };

                if let Some(mutate) = mutate {
                    assert!(mutate(CleanupTransition::Action(state, mutation, CleanupMutation::Recovery)),
                        "a recovered order has no admitted action");
                } else {
                    self.apply_published_cleanup(state, mutation);
                }
            }
        }

        if phase == AnalysisScopeExitPhase::LifecycleResolution {
            self.expire_trusted_scope(state, scope);
        }

        true
    }
}
