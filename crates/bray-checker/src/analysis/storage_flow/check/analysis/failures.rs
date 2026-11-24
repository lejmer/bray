use bray_bound_tree::{StorageCompletion, StorageEffectRow, StorageInvalidation, StorageMutation, StorageValueDestination, StorageValueRetirement};
use bray_symbols::DependencyFailureKind;
use std::sync::Arc;

use super::core::StorageFlowCollector;
use super::effect_rows::EffectRows;
use crate::CheckerRequestContext;
use crate::analysis::model::AnalysisOperationKind;
use crate::analysis::storage_flow::memory::{Effect, EffectOutcomes, Target};
use crate::analysis::storage_flow::model::{EffectPrefix, StorageFlowState};

pub(in crate::analysis::storage_flow) struct FailureEdgeEffects {
    row: Option<usize>,
    attempt: Option<PerformedFailureEffects>,
    completion: Option<StorageCompletion>,
    retirements: Vec<(Target, StorageValueRetirement)>,
}

struct PerformedFailureEffects {
    captured: std::sync::Arc<bray_bound_tree::StorageCapturedEffects>,
    performed: EffectPrefix,
    retirements: Vec<(Target, bray_bound_tree::StorageValueRetirement)>,
}

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn stage_failure_outputs(
        &self,
        state: &mut StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        invocation: super::references::InvocationOutputs<'_>,
        references: &mut [bray_bound_tree::BoundCallReference],
        stored_values: &[bray_symbols::DependencyStoredValue],
        begin: bool,
    ) {
        if begin {
            state.memory.begin_transition(occurrence);
        }

        references.sort_by_key(|reference| {
            let relation = reference.relation();

            let depth = invocation
                .storage()
                .subject(relation.exit())
                .map_or(0, |(_, path)| path.steps().len());

            (depth, relation.entry().is_some())
        });

        for reference in references {
            self.stage_invocation_reference(state, occurrence, &invocation, reference, None);
        }

        self.stage_stored_values(state, occurrence, &invocation, stored_values);
    }

    pub(super) fn capture_failure_effects(
        &mut self,
        state: &mut StorageFlowState,
        kind: AnalysisOperationKind,
        execution: &crate::analysis::guarantee::ExecutionState,
        outcomes: Option<&EffectOutcomes>,
        prefixes: &EffectRows,
        roots: &super::references::ReachedReferenceRegions,
    ) -> bool {
        if let Some(outcomes) = outcomes {
            self.record_resolved_effects(&state.memory, outcomes.possible(), None);
        }

        let occurrence = kind.occurrence();
        let mut captures = Vec::new();
        let mut can_fail = false;

        for reason in DependencyFailureKind::ALL {
            for row in self.execution.invocation_failure_rows(execution, occurrence, reason) {
                can_fail = true;

                let effects = row.and_then(|row| prefixes.get(&StorageEffectRow::Abnormal(reason, row)))
                    .cloned()
                    .or_else(|| outcomes.map(|outcomes| outcomes.shared_failure(reason)));

                let cells = row.and_then(|row| prefixes.get(&StorageEffectRow::AbnormalCells(reason, row))).cloned();

                let capture = if let Some((_, _, capture)) = captures.iter().find(|(previous, changes, _)| {
                    previous == &effects && changes == &cells
                }) {
                    Option::<std::sync::Arc<_>>::clone(capture)
                } else {
                    let capture = self.resolve_effect_prefix(state, kind, execution, effects.clone(), cells.clone(), roots).map(std::sync::Arc::new);

                    captures.push((effects, cells, capture.clone()));

                    capture
                };

                if let Some(capture) = capture {
                    state.memory.capture_failure_effects(occurrence, (reason, row), capture);
                }
            }
        }

        can_fail
    }

    pub(in crate::analysis::storage_flow) fn failure_edge_state(
        &mut self,
        source: &StorageFlowState,
        edge: bray_bound_tree::AnalysisEdgeId,
        occurrence: bray_bound_tree::BoundExecutionSite,
        kind: DependencyFailureKind,
        row: Option<usize>,
    ) -> Option<(StorageFlowState, FailureEdgeEffects)> {
        let entry = source.failure_at(occurrence);
        let admitted = entry.map_or(source, |entry| entry.state.as_ref());

        if !admitted.reachable {
            return None;
        }

        let mut failure = admitted.clone();

        failure.failure_entry = None;

        let execution = failure.execution.as_mut().expect("reachable failure retains its admitted execution");

        if !self.execution.assume_failure_row(execution, occurrence, row) { return None; }

        let admitted_rows = if row.is_none() {
            execution.admitted_call(occurrence).and_then(|entry| entry.failure_rows.clone())
        } else {
            None
        };

        let mutation = StorageMutation::InvocationFailure { occurrence, kind, edge, row };

        let write_plan = {
            let resolve = failure.memory.reached_write_resolver(self.request.semantic_values());

            self.execution.resolve_failure_writes(execution, mutation, &resolve)
        };

        let mut writes = write_plan.as_ref().map_or_else(Default::default, |(entry, writes)| {
            self.execution.bind_failure_writes(execution, entry, mutation, writes, false)
        });

        let write_plan = write_plan.map(|(_, writes)| writes);

        let commits_cleanup = matches!(occurrence, bray_bound_tree::BoundExecutionSite::Node(_))
            && occurrence.expression().and_then(|expression| self.memory.operation(expression))
            .is_some_and(|operation| crate::memory::execution::commits_buffer_cleanup(operation.kind()));

        let attempt = failure.memory.fail_transition(occurrence, (kind, row), commits_cleanup).map(|captured| {
            let (performed, retirements) = self.apply_effect_prefix(&mut failure, mutation, &captured);

            PerformedFailureEffects { captured, performed, retirements }
        });

        if let Some(attempt) = &attempt {
            writes.retire_storage(&attempt.retirements);
        }

        let completion = if let Some(plan) = write_plan {
            self.publish_failure_writes(
                &mut failure,
                occurrence,
                mutation,
                &writes,
                plan,
                commits_cleanup,
            )
        } else {
            if commits_cleanup {
                failure.memory.complete_transition(occurrence);
            }

            None
        };

        let retirements = failure.memory.take_retirements();

        failure.execution.as_mut().expect("failure retains its admitted execution").retire_captured_storage(&retirements);

        self.retain_failed_invocation_report(&mut failure, occurrence, kind, |index| {
            row.map_or_else(|| crate::ExecutionCallEvidence::admits_failure_row(admitted_rows.as_deref(), index),
                |selected| selected == index)
        });

        Some((failure, FailureEdgeEffects { row, attempt, completion, retirements }))
    }

    fn publish_failure_writes(
        &self,
        state: &mut StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        mutation: StorageMutation,
        writes: &crate::execution_guarantees::ExecutionOutputs,
        plan: bray_bound_tree::StorageFailureWrites,
        pending: bool,
    ) -> Option<StorageCompletion> {
        use bray_bound_tree::StorageFailureWrites;

        let has_logical = !writes.0.is_empty();

        let has_physical = matches!(
            &plan,
            StorageFailureWrites::Bound {
                references,
                stored_values,
                ..
            } if !references.is_empty() || !stored_values.is_empty()
        );

        if !has_logical && !has_physical {
            if pending {
                state.memory.complete_transition(occurrence);
            }

            return None;
        }

        let mut completion = StorageCompletion::default();

        for (region, value) in writes.possible_writes() {
            if let Some(bounds) = value.inline_captured_storage() {
                state.memory.include_captured_bounds(region, bounds);
            } else {
                state.memory.lose_captured_information(region);
            }
        }

        for destination in writes.destinations() {
            let StorageValueDestination::Current {
                target,
                installed,
                ..
            } = destination
            else {
                continue;
            };

            let Some(value) = writes.common_value(destination) else {
                continue;
            };

            if !*installed {
                self.publish_value_graph(
                    state,
                    target.clone(),
                    value.clone(),
                    super::references::ValuePublication::Replacement,
                );
            }
        }

        if has_physical {
            self.stage_failure_storage(state, occurrence, &plan, !pending);
        }

        if pending || has_physical {
            state.memory.complete_transition(occurrence);
        }

        for destination in writes.destinations() {
            let StorageValueDestination::Current { target, ty, .. } = destination else { continue; };

            let Some(_) = writes.common_value(destination) else { continue; };

            let snapshot = state.memory.capture_reached_value(target)
                .with_observations(self.input.footprint.typed_observations(*ty));

            self.record_current_write(state, target, *ty);
            completion.publish(destination.clone(), Arc::new(snapshot));
        }

        self.execution.complete_failed_writes(
            state.execution.as_mut().expect("reached failure retains its captured inputs"),
            mutation, writes, &completion,
        );

        completion.record_failure_writes(plan);

        Some(completion)
    }

    fn stage_failure_storage(
        &self,
        state: &mut StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        plan: &bray_bound_tree::StorageFailureWrites,
        begin: bool,
    ) {
        let bray_bound_tree::StorageFailureWrites::Bound {
            producer,
            references,
            stored_values,
            ..
        } = plan
        else {
            return;
        };

        if let Some(producer) = producer {
            self.stage_deferred_failure_storage(
                state,
                occurrence,
                producer,
                references,
                stored_values,
                begin,
            );
        } else {
            self.stage_call_failure_storage(
                state,
                occurrence,
                references,
                stored_values,
                begin,
            );
        }
    }

    fn stage_deferred_failure_storage(
        &self,
        state: &mut StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        producer: &bray_bound_tree::StorageDeferredProducer,
        references: &[bray_symbols::DependencyBorrowRelation],
        stored_values: &[bray_symbols::DependencyStoredValue],
        begin: bool,
    ) {
        let execution = state
            .execution
            .as_ref()
            .expect("reached failure retains its captured inputs");

        let (_, reached, frame) = self
            .execution
            .direct_await_body(execution, occurrence)
            .expect("deferred failure writes retain their selected frame");

        assert_eq!(reached, producer, "failure writes retain their selected producer");

        let storage = if let Some(site) = producer.local() {
            self.dependencies
                .calls()
                .get(&site)
                .expect("local deferred failure retains its call contract")
                .storage()
        } else {
            &self
                .dependencies
                .frame_callees()
                .get(
                    frame
                        .callee
                        .as_deref()
                        .expect("imported deferred failure retains its callee"),
                )
                .expect("imported deferred failure retains its body contract")
                .storage
        };

        let entry = std::sync::Arc::clone(&frame.entry);
        let has_receiver = frame.has_receiver();
        let mut prepared = storage.prepared_references(references);

        let invocation = super::references::InvocationOutputs::Frame {
            storage,
            entry: &entry,
            has_receiver,
            result: None,
        };

        self.stage_failure_outputs(
            state,
            occurrence,
            invocation,
            &mut prepared,
            stored_values,
            begin,
        );
    }

    fn stage_call_failure_storage(
        &self,
        state: &mut StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        references: &[bray_symbols::DependencyBorrowRelation],
        stored_values: &[bray_symbols::DependencyStoredValue],
        begin: bool,
    ) {
        let execution = state
            .execution
            .as_ref()
            .expect("reached failure retains its captured inputs");

        let entry = execution
            .admitted_call(occurrence)
            .expect("ordinary failure writes retain their admitted call")
            .clone();

        let contracts = self
            .dependencies
            .calls()
            .get(&occurrence)
            .expect("ordinary failure writes retain their call contract");

        let mut prepared = contracts.storage().prepared_references(references);

        let invocation = super::references::InvocationOutputs::Call {
            contracts,
            entry: Some(&entry),
            roots: &[],
        };

        self.stage_failure_outputs(
            state,
            occurrence,
            invocation,
            &mut prepared,
            stored_values,
            begin,
        );
    }

    pub(super) fn publish_failure_edge(
        &mut self,
        edge: bray_bound_tree::AnalysisEdgeId,
        occurrence: bray_bound_tree::BoundExecutionSite,
        kind: bray_symbols::DependencyFailureKind,
        record: FailureEdgeEffects,
    ) {
        use bray_bound_tree::StorageValueBoundary;

        let mutation = bray_bound_tree::StorageMutation::InvocationFailure { occurrence, kind, edge, row: record.row };

        if let Some(attempt) = record.attempt {
            self.effects.extend(std::sync::Arc::unwrap_or_clone(attempt.performed));
            self.record_outcome_mutation(mutation, std::borrow::Cow::Borrowed(&attempt.captured.invalidation));
            self.publish_retirements(mutation, StorageValueBoundary::Execution, attempt.retirements);
        } else {
            self.record_outcome_mutation(mutation, std::borrow::Cow::Owned(StorageInvalidation::Unchanged));
        }

        // An alternative with no established writes cannot inherit another branch's publication.
        self.completions.record(self.block(), mutation, record.completion.unwrap_or_default());

        self.publish_retirements(mutation, StorageValueBoundary::Retirement, record.retirements);
    }

    pub(super) fn resolve_effect_prefix(
        &mut self, state: &StorageFlowState, kind: AnalysisOperationKind,
        execution: &crate::analysis::guarantee::ExecutionState, mutations: Option<Arc<[Effect]>>,
        cell_changes: Option<Arc<[Effect]>>, roots: &super::references::ReachedReferenceRegions,
    ) -> Option<crate::analysis::storage_flow::memory::CapturedEffects> {
        let occurrence = kind.occurrence();
        let effects = mutations.as_deref();
        let mut escape_prefix = EffectPrefix::default();
        let escaped = self.address_escapes(state, execution, kind, effects, roots, &mut escape_prefix);

        if effects.is_some_and(<[_]>::is_empty)
            && cell_changes.is_none()
            && escaped.reached.is_empty()
            && escaped.published.is_empty()
            && escape_prefix.is_empty()
        {
            return None;
        }

        let mutation_roots = roots.mutation_regions(&state.memory);
        let mut invalidation = self.operation_observations(
            state, kind, Default::default(), &[], effects, Some(&mutation_roots),
            &escaped.published,
        );

        // Observing a running task does not establish a new body-entry memory snapshot.
        let observes_running_task = occurrence.expression().is_some_and(|expression| {
            self.execution.awaited_task_frame(execution, expression).is_some()
        });

        if !observes_running_task && let Some(invalidation) = &mut invalidation {
            let memory = occurrence.expression().and_then(|expression| self.memory.operation(expression));

            self.preserve_storage_values(state, (occurrence, memory), effects, invalidation);
        }

        Some(crate::analysis::storage_flow::memory::CapturedEffects {
            mutations: mutations.unwrap_or_else(|| [Effect::Unknown].into()),
            cell_changes,
            escaped: escaped.published.into_iter().collect(),
            escape_prefix: escape_prefix.iter().cloned().collect(),
            invalidation: invalidation.expect("resolved call effects have storage observations"),
        })
    }

    pub(super) fn apply_effect_prefix(
        &mut self,
        state: &mut StorageFlowState,
        mutation: bray_bound_tree::StorageMutation,
        effects: &crate::analysis::storage_flow::memory::CapturedEffects,
    ) -> (EffectPrefix, Vec<(Target, bray_bound_tree::StorageValueRetirement)>) {
        // The addresses belong to the admitted transition.
        self.check_raw_retirements(state, mutation.occurrence(), &effects.mutations);
        state.memory.escape_regions(effects.escaped.iter().cloned(), Some(mutation.occurrence()));

        let mut performed = EffectPrefix::default();

        self.record_resolved_effects(&state.memory, &effects.mutations, Some(&mut performed));

        for effect in effects.escape_prefix.iter().chain(performed.iter()) {
            if !state.effect_prefix.contains(effect) {
                // The normal proof continuation and the failure edge can both reference the performed paths.
                std::sync::Arc::make_mut(&mut state.effect_prefix).insert(effect.clone());
            }
        }

        state.memory.apply_effects(
            Some(&effects.mutations),
            &effects.invalidation,
            mutation.occurrence(),
        );

        if let Some(cells) = &effects.cell_changes {
            let cells = state.memory.checked_cell_changes(cells);

            state.memory.apply_cell_changes(cells, mutation.occurrence());
        }

        state.invalidate_observed_absence(&effects.invalidation, &self.input.footprint.discriminants);

        self.refinements.invalidate_for_operation(&mut state.refinements, self.storage,
            Some(&effects.invalidation), &self.input.footprint.discriminants);

        let execution = state.execution.as_mut().expect("reached outcome retains execution facts");

        self.execution.invalidate_observations(execution, mutation, &effects.invalidation);

        let retirements = state.memory.take_retirements();

        execution.retire_captured_storage(&retirements);

        (performed, retirements)
    }
}
