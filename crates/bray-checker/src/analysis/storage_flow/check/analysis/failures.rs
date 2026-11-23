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
    pub(super) fn capture_failure_effects(
        &mut self,
        state: &mut StorageFlowState,
        kind: AnalysisOperationKind,
        execution: &crate::analysis::guarantee::ExecutionState,
        outcomes: Option<&EffectOutcomes>,
        prefixes: &EffectRows,
        roots: &[Target],
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

        if commits_cleanup {
            failure.memory.complete_transition(occurrence);
        }

        if let Some(attempt) = &attempt {
            writes.retire_storage(&attempt.retirements);
        }

        let completion = write_plan.filter(|_| !writes.0.is_empty()).map(|plan| {
            let mut completion = self.publish_failure_writes(&mut failure, mutation, &writes);

            if !completion.publications().is_empty() {
                completion.record_failure_writes(plan);
            }

            completion
        });

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
        mutation: StorageMutation,
        writes: &crate::execution_guarantees::ExecutionOutputs,
    ) -> StorageCompletion {
        let mut completion = StorageCompletion::default();

        for (region, value) in writes.possible_writes() {
            if let Some(bounds) = value.inline_captured_storage() {
                state.memory.include_captured_bounds(region, bounds);
            } else {
                state.memory.lose_captured_information(region);
            }
        }

        for destination in writes.destinations() {
            let StorageValueDestination::Current { target, ty, installed } = destination else { continue; };

            let Some(value) = writes.common_value(destination) else { continue; };

            if !*installed {
                self.publish_value_graph(state, target.clone(), value.clone(), super::references::ValuePublication::Replacement);
            }

            let snapshot = state.memory.capture_reached_value(target)
                .with_observations(self.input.footprint.typed_observations(*ty));

            self.record_current_write(state, target);
            completion.publish(destination.clone(), Arc::new(snapshot));
        }

        self.execution.complete_failed_writes(
            state.execution.as_mut().expect("reached failure retains its captured inputs"),
            mutation, writes, &completion,
        );

        completion
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
        cell_changes: Option<Arc<[Effect]>>, roots: &[Target],
    ) -> Option<crate::analysis::storage_flow::memory::CapturedEffects> {
        let occurrence = kind.occurrence();
        let effects = mutations.as_deref();
        let mut escape_prefix = EffectPrefix::default();
        let escaped = self.address_escapes(state, execution, kind, effects, roots, &mut escape_prefix);

        if effects.is_some_and(<[_]>::is_empty) && cell_changes.is_none() && escaped.is_empty() && escape_prefix.is_empty() {
            return None;
        }

        let mut invalidation = self.operation_observations(
            state, kind, Default::default(), &[], effects, Some(roots), &escaped,
        );

        // Observing a running task does not establish a new body-entry memory snapshot.
        let observes_running_task = occurrence.expression().is_some_and(|expression| {
            self.execution.awaited_task_frame(execution, expression).is_some()
        });

        if !observes_running_task && let Some(invalidation) = &mut invalidation {
            let memory = occurrence.expression().and_then(|expression| self.memory.operation(expression));

            self.preserve_storage_values(state, memory, effects, invalidation);
        }

        Some(crate::analysis::storage_flow::memory::CapturedEffects {
            mutations: mutations.unwrap_or_else(|| [Effect::Unknown].into()),
            cell_changes,
            escaped: escaped.into_iter().collect(),
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
        state.memory.escape_regions(effects.escaped.iter().cloned());

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
