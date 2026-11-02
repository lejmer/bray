use std::collections::BTreeMap;
use std::sync::Arc;

use bray_bound_tree::{BoundExecutionSite, BoundExpressionId};

use super::super::state::ExecutionState;
use super::ExecutionFlowDomain;
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionPlace};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn deferred_alternatives(
        &self,
        state: &ExecutionState,
        operation: crate::analysis::AnalysisOperationKind,
    ) -> Option<(ExecutionCondition, Vec<super::super::deferred::DeferredValue>)> {
        use crate::analysis::{AnalysisCallPhase, AnalysisOperationKind, AnalysisSuspensionKind};

        if !matches!(operation,
            AnalysisOperationKind::Call { phase: AnalysisCallPhase::Attempt | AnalysisCallPhase::Completion, .. }
                | AnalysisOperationKind::Suspension { kind: AnalysisSuspensionKind::Await, .. }
                | AnalysisOperationKind::Bound(bray_bound_tree::AnyBoundNodeId::Expression(_)))
        {
            return None;
        }

        let expression = operation.occurrence().expression()?;

        if matches!(operation, AnalysisOperationKind::Call { phase: AnalysisCallPhase::Completion, .. })
            && (!matches!(self.request.view().expression(expression), Some(bray_bound_tree::BoundExpression::Await(_)))
                || !self.awaited_invocations(state, expression).any(|(source, _)| self.is_task_observation(source)))
        { return None; }

        let value = if let Some(bray_bound_tree::BoundExpression::Await(awaited)) =
            self.request.view().expression(expression)
        {
            self.value(state, awaited.operand())
        } else {
            let bray_bound_tree::SemanticSelection::Call(call) =
                self.semantics.selections().expression(expression)?
            else { return None; };

            crate::analysis::AnalysisTaskOperationKind::from_implementation_hook(call.implementation_hook()?)?;

            self.receiver_value(state, expression, call)
        };

        let mut value = value;
        let mut visited = std::collections::BTreeSet::new();

        loop {
            let owner = value.owner_identity();
            let deferred = state.deferred_entries(&value)?;

            let mut feasible = deferred.producers.iter().filter(|(_, candidate)| {
                super::super::deferred::possible(&candidate.selection, &state.assumptions)
            });

            let first = feasible.next();
            let second = feasible.next();

            let unknown = deferred.unknown.as_ref().filter(|guard| {
                super::super::deferred::possible(&guard.selection, &state.assumptions)
            });

            if second.is_some() || (first.is_some() && unknown.is_some()) {
                let mut alternatives = first.into_iter().chain(second).chain(feasible)
                    .map(|(source, candidate)| super::super::deferred::DeferredValue {
                        producers: [(*source, candidate.clone())].into(), unknown: None,
                    }).collect::<Vec<_>>();

                if let Some(guard) = unknown {
                    alternatives.push(super::super::deferred::DeferredValue {
                        producers: Default::default(), unknown: Some(guard.clone()),
                    });
                }

                return Some((owner.clone(), alternatives));
            }

            let (source, candidate) = first?;

            if !self.is_task_observation(*source) || !visited.insert(owner.clone()) {
                return None;
            }

            value = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0))
                .value_in(&candidate.entry.arguments)?;
        }
    }

    pub(in crate::analysis) fn select_deferred(
        &self,
        state: &mut ExecutionState,
        owner: ExecutionCondition,
        alternative: super::super::deferred::DeferredValue,
    ) {
        let guard = alternative.unknown.as_ref().map(|candidate| &candidate.selection).or_else(|| alternative.producers.values().next().map(|candidate| &candidate.selection))
            .expect("selected producer retains its ordinary guard");

        state.assumptions.extend(guard.iter().cloned());
        state.deferred_invocations.insert(owner, alternative);
    }

    pub(super) fn is_task_observation(&self, source: BoundExecutionSite) -> bool {
        source.expression().is_some_and(|expression| matches!(
            self.semantics.selections().expression(expression),
            Some(bray_bound_tree::SemanticSelection::Call(call)) if matches!(call.implementation_hook(), Some(
                bray_compiler_known::ImplementationHook::TaskJoin | bray_compiler_known::ImplementationHook::TaskCancel
            ))
        ))
    }

    pub(in crate::analysis) fn awaited_task_invocations(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<std::collections::BTreeSet<BoundExecutionSite>> {
        let (source, entry) = self.awaited_invocations(state, expression).next()?;

        if !self.is_task_observation(source) { return None; }

        let task = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)).value_in(&entry.arguments)?;

        state.deferred_entries(&task)?.known_sources(&state.assumptions)
    }

    pub(in crate::analysis) fn capture_deferred_reports(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        snapshot: impl FnOnce() -> crate::dependency::ReportDependencies,
    ) {
        let value = self.value(state, expression);

        let Some(deferred) = state.deferred_invocations.get_mut(value.owner_identity()) else { return; };

        let mut snapshot = Some(snapshot);
        let mut captured = None;

        for reports in deferred.producers.iter_mut().filter(|(source, _)| {
            self.dependencies.calls()[source].deferred_reports().is_some_and(|reports| {
                [reports.completed(), reports.abnormal()].into_iter().flatten()
                    .any(|contract| !contract.requirements().is_empty())
            })
        }).map(|(_, candidate)| &mut candidate.reports)
            .chain(deferred.unknown.iter_mut().map(|candidate| &mut candidate.reports))
        {
            if reports.is_none() {
                *reports = Some(Arc::clone(captured.get_or_insert_with(|| Arc::new(
                    snapshot.take().expect("one frame capture computes its report snapshot once")()
                ))));
            }
        }
    }

    pub(in crate::analysis) fn awaited_report_dependencies(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<Arc<crate::dependency::ReportDependencies>> {
        let Some(bray_bound_tree::BoundExpression::Await(awaited)) = self.request.view().expression(expression)
            else { return None; };

        let value = self.value(state, awaited.operand());
        let deferred = state.deferred_entries(&value)?;
        let mut reports = None;

        for captured in deferred.producers.values().map(|candidate| (&candidate.selection, &candidate.reports))
            .chain(deferred.unknown.iter().map(|candidate| (&candidate.selection, &candidate.reports)))
            .filter(|(guard, _)| super::super::deferred::possible(guard, &state.assumptions))
            .map(|(_, reports)| reports.as_ref().expect("formed future retains its captured report dependencies"))
        {
            if let Some(previous) = &mut reports {
                crate::dependency::ReportDependencies::merge(previous, captured);
            } else { reports = Some(Arc::clone(captured)); }
        }

        reports
    }

    pub(super) fn activate_task(&self, state: &mut ExecutionState, expression: BoundExpressionId) {
        if self.started_future(state, expression).is_none() {
            return;
        }

        let task = self.value(state, expression);
        let mut retired = std::collections::BTreeSet::new();

        // Execution moves captured authority into the child run. Its immutable entry
        // remains available only to the body's normal Completed result.
        for (carrier, witness) in &mut state.witnesses {
            if !carrier.is_owned_projection_of(&task) {
                continue;
            }

            for condition in witness.conditions.keys() {
                condition.clone().assume(true, &mut retired);
            }

            if witness.available {
                Arc::make_mut(witness).available = false;
            }
        }

        state.exclude_live_witnesses(&mut retired);

        state
            .trusted_assumptions
            .retain(|fact| !retired.contains(fact));
    }

    pub(super) fn set_awaited_execution(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        pending: bool,
    ) {
        let Some(bray_bound_tree::BoundExpression::Await(awaited)) =
            self.request.view().expression(expression)
        else {
            return;
        };

        let value = self.value(state, awaited.operand());

        if let Some(entries) = state.deferred_invocations.get_mut(value.owner_identity()) {
            for candidate in entries.producers.values_mut() {
                Arc::make_mut(&mut candidate.entry).pending_execution = pending;
            }
        }
    }

    pub(super) fn capture_deferred(
        &self,
        state: &mut ExecutionState,
        invocation: BoundExecutionSite,
        result: &ExecutionCondition,
    ) {
        let Some(entry) = state
            .entries
            .get(&invocation)
            .filter(|entry| entry.pending_execution)
        else {
            return;
        };

        // A live frame owns an immutable entry snapshot independently of later factory evaluations.
        let mut entry = entry.clone();
        let mut values = BTreeMap::new();
        let mut identities = BTreeMap::new();
        let mut ownership = Vec::new();

        for ordinal in entry.owned_inputs.iter() {
            let Some(value) = ExecutionPlace::argument(*ordinal).value_in(&entry.arguments) else {
                continue;
            };

            let identity = ExecutionCondition::CapturedInput {
                owner: Arc::new(result.owner_identity().clone()),
                invocation,
                ordinal: *ordinal,
            };

            let captured = match value {
                ExecutionCondition::Input(_) => identity,
                _ => super::joins::owned_snapshot(&value, identity, &mut {
                    ExecutionCondition::WORK_LIMIT
                }),
            };

            ownership.push((captured.clone(), state.owned_result(&value)));

            super::merge::result_replacements(
                &value,
                &captured,
                &Default::default(),
                &Default::default(),
                &mut values,
                &mut identities,
            );
        }

        if let Some(renamed) = entry.joined_values(&values, &identities) {
            entry = renamed;
        }

        state.join_values(&values, &identities);

        // Copyable contents get private observations without renaming their caller
        // or transferring its authority. Track origins only while unchanged;
        // projected capture facts survive later source mutation.
        for (place, value) in entry.arguments.clone() {
            let crate::execution_guarantees::ExecutionInput::Argument(ordinal) = place.root else {
                continue;
            };

            if !place.projections.is_empty()
                || entry.owned_inputs.contains(&ordinal)
                || value.observes_live(None)
                || matches!(
                    value,
                    ExecutionCondition::Boolean(_)
                        | ExecutionCondition::Literal(_)
                        | ExecutionCondition::Variant(_)
                        | ExecutionCondition::Unknown
                )
            {
                continue;
            }

            let captured = entry
                .copy_sources
                .iter()
                .find(|(_, source)| **source == value)
                .and_then(|(previous, _)| previous.value_in(&entry.arguments))
                .unwrap_or_else(|| {
                    let identity = ExecutionCondition::CapturedInput {
                        owner: Arc::new(result.owner_identity().clone()),
                        invocation,
                        ordinal,
                    };

                    super::joins::owned_snapshot(&value, identity.clone(), &mut {
                        ExecutionCondition::WORK_LIMIT
                    })
                    .with_snapshots(&[(value.owner_identity().clone(), identity)].into())
                });

            let snapshots = [(value.clone(), captured.clone())].into();

            for argument in entry.arguments.values_mut() {
                *argument = argument.with_snapshots(&snapshots);
            }

            entry.copy_sources.insert(place.clone(), value);
            entry.arguments.insert(place, captured);
        }

        let captured = entry.captured_facts(&entry.assumptions);

        state.assumptions.extend(captured.iter().cloned());
        entry.assumptions.extend(captured);

        for (value, ownership) in ownership {
            if !ownership.is_empty() {
                state.owned_results.insert(value, ownership);
            }
        }

        state
            .deferred_invocations
            .entry(result.owner_identity().clone())
            .or_default()
            .producers
            .insert(
                invocation,
                super::super::deferred::DeferredInvocation {
                    entry: Arc::new(entry),
                    reports: None,
                    selection: super::super::deferred::selection(&state.assumptions),
                },
            );

        state
            .entries
            .get_mut(&invocation)
            .expect("formed future retains its entry until publication")
            .pending_execution = false;
    }

    pub(in crate::analysis) fn awaited_invocations<'s>(
        &self,
        state: &'s ExecutionState,
        expression: BoundExpressionId,
    ) -> impl Iterator<Item = (BoundExecutionSite, &'s Arc<crate::ExecutionCallEvidence>)> {
        let entries = match self.request.view().expression(expression) {
            Some(bray_bound_tree::BoundExpression::Await(awaited)) => {
                state.deferred_entries(&self.value(state, awaited.operand()))
            }
            _ => None,
        };

        entries
            .into_iter()
            .flat_map(|entries| entries.producers.iter())
            .filter(|(_, candidate)| {
                super::super::deferred::possible(&candidate.selection, &state.assumptions)
            })
            .map(|(source, candidate)| (*source, &candidate.entry))
    }

    pub(in crate::analysis) fn awaited_unknown(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> bool {
        let Some(bray_bound_tree::BoundExpression::Await(awaited)) =
            self.request.view().expression(expression)
        else {
            return false;
        };

        state
            .deferred_entries(&self.value(state, awaited.operand()))
            .is_none_or(|entries| entries.unknown_possible(&state.assumptions))
    }

    pub(super) fn remember_deferred_value(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
    ) {
        let Some(ty) = self.semantics.types().expression(expression) else {
            return;
        };

        if !matches!(
            crate::representation::type_representation(self.request, ty.ty()),
            Some(
                bray_compiler_known::RepresentationRole::Future
                    | bray_compiler_known::RepresentationRole::Task
            )
        ) {
            return;
        }

        let value = self.value(state, expression);

        if value != ExecutionCondition::Unknown {
            state
                .deferred_invocations
                .entry(value.owner_identity().clone())
                .or_insert_with(|| {
                    super::super::deferred::DeferredValue::unknown(&state.assumptions)
                });
        }
    }

    pub(super) fn complete_deferred(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        observations: &BTreeMap<
            BoundExecutionSite,
            std::collections::BTreeSet<bray_bound_tree::StoragePath>,
        >,
    ) -> bool {
        let Some(bray_bound_tree::BoundExpression::Await(awaited)) =
            self.request.view().expression(expression)
        else {
            return false;
        };

        // Completion owns its producer alternatives while mutating the flow environment.
        let Some(deferred) = state
            .deferred_entries(&self.value(state, awaited.operand()))
            .cloned()
        else {
            return false;
        };

        if deferred.producers.is_empty() {
            return false;
        }

        let root = ExecutionCondition::Expression(expression);

        if let Some((mut completed, value)) = self.complete_producers(
            state,
            &root,
            expression,
            &deferred,
            observations,
        ) {
            completed.expressions.insert(expression, value);
            *state = completed;
        }

        true
    }

    fn complete_producers(
        &self,
        state: &ExecutionState,
        root: &ExecutionCondition,
        expression: BoundExpressionId,
        deferred: &super::super::deferred::DeferredValue,
        observations: &BTreeMap<
            BoundExecutionSite,
            std::collections::BTreeSet<bray_bound_tree::StoragePath>,
        >,
    ) -> Option<(ExecutionState, ExecutionCondition)> {

        // An opaque alternative contributes no producer-specific completion evidence.
        let mut completed = deferred
            .unknown
            .as_ref()
            .filter(|guard| super::super::deferred::possible(&guard.selection, &state.assumptions))
            .map(|guard| {
                let mut branch = state.clone();

                branch.assumptions.extend(guard.selection.iter().cloned());

                (branch, root.clone())
            });

        for (&source, candidate) in &deferred.producers {
            if !super::super::deferred::possible(&candidate.selection, &state.assumptions) {
                continue;
            }

            let entry = &candidate.entry;

            if !self.dependencies.calls()[&source].deferred_completion() {
                continue;
            }

            // Each possible producer completes in an independent environment before ordinary flow joins it.
            let mut branch = state.clone();

            branch.assumptions.extend(candidate.selection.iter().cloned());
            branch.assumptions.extend(entry.assumptions.iter().cloned());

            if !self.owned_normal_completion(&branch.assumptions, source, &entry, root) {
                continue;
            }

            let mut entry = entry.as_ref().clone();

            entry.pending_execution = false;
            branch.entries.insert(source, entry);

            let observed_task = self.is_task_observation(source)
                .then(|| branch.entries.get(&source))
                .flatten()
                .and_then(|entry| ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)).value_in(&entry.arguments))
                .and_then(|task| branch.deferred_entries(&task))
                .cloned();

            let value = if let Some(task) = observed_task {
                self.complete_task_observation(
                    &mut branch,
                    &task,
                    root.clone(),
                    expression,
                    observations,
                )
            } else {
                root.clone()
            };

            // Resolve body ownership before observation consumes the Task's captures.
            let value = self.complete_trusted_call(&mut branch, source, value, observations);

            if let Some((previous, previous_value)) = &mut completed {
                *previous_value = super::merge_returned(
                    previous,
                    &branch,
                    previous_value,
                    &value,
                    root,
                    expression,
                );
            } else {
                completed = Some((branch, value));
            }
        }

        completed
    }

    fn complete_task_observation(
        &self,
        state: &mut ExecutionState,
        deferred: &super::super::deferred::DeferredValue,
        root: ExecutionCondition,
        expression: BoundExpressionId,
        observations: &BTreeMap<
            BoundExecutionSite,
            std::collections::BTreeSet<bray_bound_tree::StoragePath>,
        >,
    ) -> ExecutionCondition {
        let (variant, field) = crate::representation::union_payload(
            self.request,
            "RunResultVariant0Completed",
            "RunResultVariant0CompletedValue",
        );

        let projection =
            bray_bound_tree::StorageProjection::ActiveUnionPayloadField { variant, field };

        let completed = ExecutionCondition::VariantTest(variant, Arc::new(root.clone()));

        match completed.prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT }) {
            Some(false) => return root,
            Some(true) => {
                let payload = root.clone().project(projection);

                if let Some((normal, value)) = self.complete_producers(state, &payload, expression, deferred, observations) {
                    *state = normal;

                    return ExecutionCondition::Constructed(Arc::new(root), [(projection, value)].into());
                }

                return root;
            }
            None => {}
        }

        // Observation may also return a panic or cancellation. Only Completed owns the body's normal result.
        let mut normal = state.clone();

        completed.clone().assume(true, &mut normal.assumptions);

        let payload = root.clone().project(projection);
        let mut abnormal = state.clone();

        completed.assume(false, &mut abnormal.assumptions);

        let Some((mut normal, value)) = self.complete_producers(
            &normal,
            &payload,
            expression,
            deferred,
            observations,
        ) else {
            *state = abnormal;

            return root;
        };

        let value =
            ExecutionCondition::Constructed(Arc::new(root.clone()), [(projection, value)].into());

        let joined =
            super::merge_returned(&mut normal, &abnormal, &value, &root, &root, expression);

        *state = normal;

        joined
    }
}
