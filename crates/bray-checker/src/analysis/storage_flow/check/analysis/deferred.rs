use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_bound_tree::{
    BoundCallStorage, BoundDependencyRequirementKind, BoundDependencySubject, BoundExecutionSite,
    ReportDependencies,
    StorageMutationTarget, StoragePath, StorageValueConsumptionPhase,
};
use bray_symbols::{DependencySubject, DependencySubjectRoot};

use super::core::StorageFlowCollector;
use super::effect_rows::{EffectRows, SelectedEffectRows};
use crate::analysis::storage_flow::memory::{Effect, EffectOutcomes, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::CheckerRequestContext;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn awaited_dependencies(
        &self,
        state: &StorageFlowState,
        operation: crate::analysis::AnalysisOperationKind,
    ) -> (Vec<BoundDependencySubject>, Vec<(BoundDependencySubject, BoundDependencyRequirementKind)>) {
        if !self.publish {
            return Default::default();
        }

        let Some(expression) = operation.awaited_expression() else { return Default::default(); };

        let execution = state.execution.as_ref().expect("a reached suspension retains execution facts");

        let Some(value) = self.execution.frame_execution_value(execution, operation) else { return Default::default(); };

        let Some(frame) = value.deferred_value() else { return Default::default(); };

        let mut inputs = self.storage.value_consumptions(expression.into(), StorageValueConsumptionPhase::Admission);

        let Some(input) = inputs.next() else {
            assert!(state.recovered || self.storage.is_recovered() || frame.producers.is_empty(),
                "awaited frame retains its planned input: {expression:?}");

            return Default::default();
        };

        assert!(inputs.next().is_none(), "await admits one evaluated input: {expression:?}");

        let mut retained = Vec::new();

        let unavailable = self.check_frame_execution_requirements(
            state, frame, &execution.ordinary_proof(), BoundDependencySubject::Storage(input.root),
            |subject| retained.push(subject),
        );

        (retained, unavailable)
    }

    fn check_frame_execution_requirements(
        &self,
        state: &StorageFlowState,
        frame: &crate::ExecutionDeferredValue,
        proof: &crate::execution_guarantees::ExecutionProof<'_>,
        owner: BoundDependencySubject,
        mut retain: impl FnMut(BoundDependencySubject),
    ) -> Vec<(BoundDependencySubject, BoundDependencyRequirementKind)> {
        let mut unavailable = Vec::new();

        for (source, producer) in frame.producers.iter().filter(|(_, producer)| {
            crate::execution_guarantees::deferred_possible(&producer.selection, proof)
        }) {
            self.execution.visit_frame_execution_requirements(source, producer, proof, owner, |storage, formal, subject, kind| {
                if let Some((ty, path)) = storage.subject(formal).filter(|(_, path)| !path.steps().is_empty()) {
                    let targets = if formal.subject_root().is_static() {
                        self.static_location(&DependencySubject::root(formal.subject_root()))
                            .and_then(|start| state.memory.reference_locations(self.request.semantic_values(), start, path, None))
                            .map(|targets| (targets, None))
                    } else {
                        formal.subject_root().input_ordinal(producer.has_receiver())
                            .and_then(|ordinal| producer.entry.argument_storage(ordinal))
                            .and_then(|captured| state.memory.captured_dependency_targets(
                                self.request.semantic_values(), captured, path,
                            ))
                    };

                    let Some((targets, loan)) = targets.filter(|(targets, _)| !targets.is_empty()) else {
                        unavailable.push((subject.unwrap_or(owner), kind));
                        return;
                    };

                    let values = self.request.semantic_values();
                    let borrowed = matches!(values.type_data(ty).as_ref(), bray_symbols::TypeData::Borrow { .. });

                    for target in &targets {
                        let loan = if borrowed { state.memory.reached_borrow_capability(target) } else { loan };

                        let subject = match kind {
                            BoundDependencyRequirementKind::BorrowCapabilityActive(_)
                            | BoundDependencyRequirementKind::ExclusiveMutationAuthority => loan.map(BoundDependencySubject::BorrowCapability),
                            _ => None,
                        }.or_else(|| target.dependency_subject(values, self.storage)).unwrap_or(owner);

                        retain(subject);

                        if !super::dependencies::reached_dependency_is_satisfied(
                            values, self.storage, state, target, ty, loan, kind,
                        ) {
                            unavailable.push((subject, kind));
                        }
                    }

                    return;
                }

                let Some(subject) = subject else {
                    unavailable.push((owner, kind));
                    return;
                };

                retain(subject);

                if !super::dependencies::dependency_subject_is_satisfied(
                    self.request.semantic_values(), self.storage, state, subject, kind,
                ) {
                    unavailable.push((subject, kind));
                }
            });
        }

        unavailable
    }

    fn check_started_dependencies(
        &mut self,
        state: &StorageFlowState,
        expression: bray_bound_tree::BoundExpressionId,
        frame: &crate::ExecutionDeferredValue,
        proof: &crate::execution_guarantees::ExecutionProof<'_>,
    ) {
        if frame.producers.is_empty() {
            return;
        }

        let Some(access) = self.storage.invocation_input(expression, bray_symbols::SymbolOrdinal::new(0)) else {
            assert!(state.recovered || self.storage.is_recovered(),
                "a selected task start stages its owned receiver: {expression:?}");

            return;
        };

        let unavailable = self.check_frame_execution_requirements(
            state, frame, proof, BoundDependencySubject::StorageAccess(access), |_| {},
        );

        if !unavailable.is_empty() {
            self.diagnostics.record_task_start(expression, unavailable);
        }
    }

    pub(super) fn publish_frame_captures(
        &self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
    ) {
        let Some(expression) = occurrence.expression() else { return; };

        let Some(result) = self.expression_result_storage(expression) else { return; };

        let execution = state.execution.as_ref().expect("a completed value has its paired execution state");
        let value = self.execution.value(execution, expression);
        let frames = value.deferred_values();

        if frames.is_empty() {
            return;
        }

        let proof = execution.ordinary_proof();

        let captures = frames.into_iter().flat_map(|frame| frame.producers.values())
            .filter(|producer| crate::execution_guarantees::deferred_possible(&producer.selection, &proof))
            .flat_map(|producer| producer.entry.positional_arguments())
            .filter_map(|(_, value)| value.inline_captured_storage());

        state.memory.capture_inputs(result, captures);
    }

    pub(super) fn stage_frame_storage_outputs(
        &self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
        result: &StoragePath,
        frame: &crate::ExecutionDeferredValue,
        proof: &crate::execution_guarantees::ExecutionProof<'_>,
    ) {
        if frame.unknown_possible(proof) {
            state.memory.begin_transition(occurrence);
            return;
        }

        let mut completed: Option<bray_bound_tree::StorageCapturedTransition> = None;

        for (source, producer) in &frame.producers {
            if !crate::execution_guarantees::deferred_possible(&producer.selection, proof) {
                continue;
            }

            let Some((storage, rows)) = self.execution.frame_storage_outcomes(source, producer) else {
                completed.get_or_insert_with(Default::default).merge(&Default::default());
                continue;
            };

            let outcomes = rows.iter().enumerate()
                .filter(|(row, _)| producer.entry.outputs.is_admitted(*row))
                .map(|(_, outcome)| outcome);

            if !rows.is_empty() && outcomes.clone().next().is_none() {
                continue;
            }

            let mut references = storage.reference_outputs(outcomes.clone());

            let invocation = super::references::InvocationOutputs::Frame {
                storage,
                entry: &producer.entry,
                has_receiver: producer.has_receiver(),
                result,
            };

            self.stage_invocation_outputs(state, occurrence, invocation, &mut references, outcomes, None);

            let prepared = state.memory.take_transition(occurrence)
                .expect("a selected frame has a prepared output transition");

            if let Some(previous) = &mut completed { previous.merge(&prepared); }
            else { completed = Some(prepared); }
        }

        state.memory.stage_transition(occurrence, completed.unwrap_or_default());
    }

    pub(super) fn stage_deferred_report_payloads(
        &self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
        result: StoragePath,
        frame: &crate::ExecutionDeferredValue,
        proof: &crate::execution_guarantees::ExecutionProof<'_>,
    ) {
        use crate::execution_guarantees::{FrameReportOwner, deferred_possible};

        let producers = frame.producers.iter()
            .filter(|(_, producer)| deferred_possible(&producer.selection, proof));

        let mut payloads = BTreeMap::<_, (Target, Arc<ReportDependencies>)>::new();
        let root = Target::storage(result);

        for (source, producer) in producers.clone() {
            let Some((storage, _)) = self.execution.frame_storage_outcomes(source, producer) else { continue; };

            for (owner, reports) in &producer.reports {
                let FrameReportOwner::Payload(destination) = owner else { continue; };

                let Some((_, path)) = storage.subject(destination).filter(|(_, path)| path.steps().is_empty()) else { continue; };

                let Some(report) = reports.admitted(|row| producer.entry.outputs.is_admitted(row)) else { continue; };

                payloads.entry(destination).and_modify(|(_, support)| {
                    support.merge(&report);
                }).or_insert_with(|| (root.project(path.projections()), report));
            }
        }

        for (destination, (payload, report)) in payloads {
            let complete = !frame.unknown_possible(proof) && producers.clone().all(|(source, producer)| {
                let Some((_, rows)) = self.execution.frame_storage_outcomes(source, producer) else { return false; };

                !rows.is_empty() && rows.iter().enumerate().all(|(row, outcome)| {
                    !producer.entry.outputs.is_admitted(row)
                        || outcome.accounts_for_report_payload(destination, DependencySubject::contains)
                })
            });

            state.memory.stage_report(occurrence, root.clone(), payload, report, complete);
        }
    }

    pub(super) fn start_frame_effects(
        &mut self,
        state: &StorageFlowState,
        execution: &mut crate::analysis::guarantee::ExecutionState,
        operation: crate::analysis::AnalysisOperationKind,
    ) {
        let Some(value) = self.execution.frame_execution_value(execution, operation) else { return; };

        let Some(frame) = value.deferred_value() else { return; };

        let proof = execution.ordinary_proof();

        if self.publish && let Some(expression) = operation.explicit_attempt_expression() {
            self.check_started_dependencies(state, expression, frame, &proof);
        }

        let memory = &state.memory;
        let mut started = Arc::clone(frame);

        for (source, candidate) in &frame.producers {
            if !crate::execution_guarantees::deferred_possible(&candidate.selection, &proof) { continue; }

            if self.publish
                && let Some(call) = self.execution.deferred_behavior(source, candidate, operation.occurrence())
            {
                self.deferred_calls.entry(operation.occurrence()).or_default().insert(call);
            }

            let (effects, rows) = if self.execution.is_task_observation(source.clone()) {
                let effects = crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)).value_in(&candidate.entry.arguments).and_then(|task| {
                    let task = task.deferred_value()?;

                    super::effects::task_effects(task.effects(&proof), task.cleanup_effects(false, &proof)?)
                });

                (effects, None)
            } else {
                self.bind_frame_effects(memory, source, candidate)
                    .map_or((None, None), |(effects, rows)| (Some(effects), rows))
            };

            let previous = candidate.transition.as_deref();

            let cleanup = if previous.is_some_and(|transition| {
                transition.captured_cleanup.is_some() && transition.result_cleanup.is_some()
            }) {
                None
            } else {
                self.frame_cleanup_contract(source, candidate).map(|(storage, count, [captures, result])| {
                    [
                        self.resolve_frame_cleanup(memory, candidate, storage, captures, Some(count)).into_iter().collect(),
                        self.resolve_frame_cleanup(memory, candidate, storage, result, None).into_iter().collect(),
                    ]
                })
            };

            if cleanup.is_none() && previous.and_then(|transfer| transfer.effects.as_ref()) == effects.as_ref()
                && previous.and_then(|transfer| transfer.effect_rows.as_ref()) == rows.as_ref()
            {
                continue;
            }

            let candidate = std::sync::Arc::make_mut(&mut started).producers.get_mut(source)
                .expect("execution starts the selected frame");

            let transition = candidate.transition.get_or_insert_with(|| std::sync::Arc::new(
                bray_bound_tree::StorageDeferredTransition::default()
            ));

            let transfer = Arc::make_mut(transition);

            transfer.effects = effects;
            transfer.effect_rows = rows;

            if let Some([captures, result]) = cleanup {
                transfer.captured_cleanup = Some(captures);
                transfer.result_cleanup = Some(result);
            }
        }

        if !std::sync::Arc::ptr_eq(&started, frame) {
            execution.replace_deferred_value(frame, Some(started));
        }
    }

    fn bind_frame_effects(
        &self, memory: &crate::analysis::storage_flow::memory::State,
        source: &bray_bound_tree::StorageDeferredProducer,
        candidate: &crate::execution_guarantees::ExecutionDeferredInvocation,
    ) -> Option<(EffectOutcomes, Option<EffectRows>)> {
        let entry = &candidate.entry;
        let mut selection;

        let (templates, routes, rows, count) = if let Some(site) = source.local() {
            let call = self.dependencies.calls().get(&site)?;
            let rows = call.captured_values();

            selection = SelectedEffectRows::new(!rows.is_empty());

            selection.select_failures(
                call.deferred_reports()?.failures(),
                |row| entry.admits_failure(row),
                |_| entry.failure_rows.is_some(),
            );

            (call.deferred_effect_outcomes()?, call.storage(), rows, call.input_count())
        } else {
            let callee = candidate.callee.as_deref()?;
            let body = self.dependencies.frame_callees().get(callee)?;
            let summary = &body.summary;
            let rows = summary.captured_values();

            selection = SelectedEffectRows::new(!rows.is_empty());

            selection.select_failures(
                summary.failure_outcomes(),
                |row| entry.admits_failure(row),
                |_| entry.failure_rows.is_some(),
            );

            (summary.effect_outcomes(), &body.storage, rows, body.capture_types.len())
        };

        for (index, row) in rows.iter().enumerate().filter(|(index, _)| {
            entry.outputs.is_admitted(*index)
        }) {
            selection.select_normal(index, row, true);
        }

        let (effects, rows) = selection.bind(templates, |effect| {
            self.resolve_effects(memory, routes, std::slice::from_ref(effect), || entry.argument_regions(count), |subject, indirect| {
                self.frame_effect_targets(memory, candidate, routes, subject, indirect)
            })
        });

        Some((effects, (!rows.is_empty()).then_some(rows)))
    }

    fn frame_effect_targets(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        candidate: &crate::ExecutionDeferredInvocation,
        storage: &BoundCallStorage,
        subject: &DependencySubject,
        indirect: bool,
    ) -> Option<Vec<StorageMutationTarget>> {
        let values = self.request.semantic_values();
        let root = subject.subject_root();

        let (_, path) = storage.subject(subject)?;

        if root.is_static() {
            let start = self.static_location(&DependencySubject::root(root))?;

            return memory.reference_effect_targets(values, start, Some(path), indirect);
        }

        let captured = if root == DependencySubjectRoot::Result {
            candidate.entry.outputs.common_value(&bray_bound_tree::StorageValueDestination::Result)?
                .inline_captured_storage()?
        } else {
            let ordinal = root.input_ordinal(candidate.has_receiver())?;

            candidate.entry.argument_storage(ordinal)?
        };

        memory.captured_effect_targets(values, captured, path, indirect)
    }

    pub(super) fn inactive_frame_cleanup(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        frame: &crate::ExecutionDeferredValue,
        proof: &crate::execution_guarantees::ExecutionProof<'_>,
    ) -> Option<Vec<Effect>> {
        if frame.unknown_possible(proof) { return None; }

        let mut cleanup = BTreeSet::new();

        for (source, candidate) in &frame.producers {
            if !crate::execution_guarantees::deferred_possible(&candidate.selection, proof) { continue; }

            let (storage, count, [captures, _]) = self.frame_cleanup_contract(source, candidate)?;

            cleanup.extend(self.resolve_frame_cleanup(memory, candidate, storage, captures, Some(count)));
        }

        Some(cleanup.into_iter().collect())
    }

    fn frame_cleanup_contract(
        &self,
        source: &bray_bound_tree::StorageDeferredProducer,
        candidate: &crate::ExecutionDeferredInvocation,
    ) -> Option<(&BoundCallStorage, usize, [&[bray_symbols::DependencyStorageEffect]; 2])> {
        if let Some(site) = source.local() {
            let call = self.dependencies.calls().get(&site)?;

            Some((call.storage(), call.input_count(), [call.deferred_capture_cleanup()?, call.deferred_result_cleanup()?]))
        } else {
            let body = self.dependencies.frame_callees().get(candidate.callee.as_deref()?)?;

            Some((&body.storage, body.capture_types.len(), [&body.capture_cleanup, &body.result_cleanup]))
        }
    }

    fn resolve_frame_cleanup(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        candidate: &crate::ExecutionDeferredInvocation,
        storage: &BoundCallStorage,
        templates: &[bray_symbols::DependencyStorageEffect],
        capture_count: Option<usize>,
    ) -> Vec<Effect> {
        self.resolve_effects(
            memory,
            storage,
            templates,
            || match capture_count {
                Some(count) => candidate.entry.argument_regions(count),
                None => candidate.entry.outputs.common_value(&bray_bound_tree::StorageValueDestination::Result)
                    .and_then(crate::ExecutionCondition::captured_input_regions)
                    .map(bray_base::sorted_unique_shared_slice),
            },
            |subject, indirect| {
                if !subject.subject_root().is_static() && !indirect
                    && storage.subject(subject).is_some_and(|(_, path)| path.steps().is_empty())
                {
                    // Inline disposal reaches caller memory through separately captured descriptors.
                    return Some(Vec::new());
                }

                self.frame_effect_targets(memory, candidate, storage, subject, indirect)
            },
        )
    }

}
