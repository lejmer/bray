use std::collections::BTreeMap;
use std::sync::Arc;

use bray_bound_tree::{
    AsyncCleanupPhases, StorageAccessId, StorageCleanupAction, StorageCleanupActionKind,
    StorageCleanupEntry, StorageMutation, StoragePath,
};
use bray_symbols::TypeAssociatedLifecycleSlot;

use super::plan::{Lifecycle, Step};
use super::selection::{ReplacementPath, cleanup_path};
use super::transition::{CleanupMutation, CleanupObservation, CleanupTransition, LifecycleInvocation, Mutation, Observer};
use crate::analysis::guarantee::{ExecutionFlowDomain, ExecutionState};
use crate::analysis::model::{AnalysisOperation, AnalysisOperationKind};
use crate::analysis::outcomes::CleanupProgress;
use crate::{CheckerRequestContext, ExecutionCallEvidence, ExecutionPlace};

fn part_absent(
    access: StorageAccessId,
    part: usize,
    phase: AsyncCleanupPhases,
    owner: Option<&StoragePath>,
    entry: Option<&StorageCleanupEntry>,
    mutate: &mut Option<&mut Mutation<'_>>,
) -> bool {
    if let Some(mutate) = mutate {
        return mutate(CleanupTransition::PartAbsent { access, part, phase, owner });
    }

    entry.is_some_and(|entry| entry.part_absent(access, part, phase))
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    fn published_cleanup(&self) -> &bray_bound_tree::StorageFlow {
        self.checked_body
            .expect("execution cleanup consumes checked body semantics")
            .storage_flow()
    }

    pub(super) fn apply_published_cleanup(&self, state: &mut ExecutionState, mutation: StorageMutation) {
        let flow = self.published_cleanup();

        state.retire_captured_storage(flow.capture_retirements(mutation, bray_bound_tree::StorageValueBoundary::Execution));

        self.invalidate_observations(
            state,
            mutation,
            flow.outcome_mutation(mutation).unwrap_or_else(|| panic!("selected cleanup {mutation:?} has no checked transition")),
        );

        if let Some(completion) = flow.completion(mutation) {
            self.refresh_storage_observations(state, completion.references());
        }
    }

    fn cleanup_action_reached(&self, action: StorageCleanupAction) -> bool {
        let flow = self.published_cleanup();

        flow.outcome_mutation(StorageMutation::Cleanup(action)).is_some()
            || flow.cleanup_outcomes(action).next().is_some()
            || flow.cleanup_failures(action).next().is_some()
    }

    fn replay_cleanup_action(
        &self,
        state: &mut ExecutionState,
        action: StorageCleanupAction,
        remaining: &[StorageAccessId],
        invocation: Option<(&ExecutionCallEvidence, &bray_symbols::DependencyContractTemplateData)>,
        observe: &mut Option<&mut Observer<'_>>,
    ) -> bool {
        let flow = self.published_cleanup();

        if let Some(observe) = observe {
            for (mutation, _) in flow.cleanup_failures(action) {
                let StorageMutation::CleanupFailure { kind, row, .. } = mutation else {
                    unreachable!("checked fault iterator has one action's failures");
                };

                let Some(remainder) = flow.cleanup_remainders(mutation)
                    .find(|remainder| remainder.accesses.as_ref() == remaining)
                else {
                    continue;
                };

                let mut failed = state.clone();

                if invocation.is_some_and(|(evidence, _)| !self.assume_cleanup_failure(&mut failed, evidence, row)) {
                    continue;
                }

                if let Some((entry, _)) = invocation {
                    self.replay_lifecycle_completion(&mut failed, mutation, entry);
                } else {
                    self.apply_published_cleanup(&mut failed, mutation);
                }

                failed.cleanup = Some(CleanupProgress::during(state.cleanup.as_ref(), kind, remainder.clone()));
                observe(CleanupObservation::Continuation(Box::new(failed)));
            }
        }

        let normal = StorageMutation::Cleanup(action);

        if flow.outcome_mutation(normal).is_some() {
            self.apply_published_cleanup(state, normal);

            return true;
        }

        let Some((entry, contract)) = invocation else { return false; };

        let mut rows = flow.cleanup_outcomes(action).filter_map(|(mutation, row)| {
            let outcome = &contract.captured_values()[row];

            self.cleanup_normal_guard(state, entry, outcome).map(|guard| (mutation, guard))
        });

        let Some(first) = rows.next() else { return false; };

        let apply = |state: &mut ExecutionState, (mutation, guard)| {
            self.assume_cleanup_guard(state, guard);
            self.replay_lifecycle_completion(state, mutation, entry);
        };

        let Some(second) = rows.next() else {
            drop(rows);
            apply(state, first);

            return true;
        };

        let mut first_branch = state.clone();

        apply(&mut first_branch, first);

        let mut merged = Some(first_branch);

        for row in std::iter::once(second).chain(rows) {
            let mut branch = state.clone();

            apply(&mut branch, row);

            super::super::flow::merge_execution(
                &mut merged, &Some(branch), super::super::flow::ExecutionJoin::Boundary,
            );
        }

        *state = merged.expect("a reached cleanup row supplies the next execution state");

        true
    }

    fn replay_lifecycle_completion(
        &self,
        state: &mut ExecutionState,
        mutation: StorageMutation,
        entry: &ExecutionCallEvidence,
    ) {
        use bray_bound_tree::{StorageValueBoundary, StorageValueDestination};

        let flow = self.published_cleanup();

        let publication = flow.completion(mutation).and_then(|completion| {
            let (input, transition) = completion.lifecycle_receiver()?;

            let mut values = self.bind_cleanup_receiver_outputs(state, entry, mutation, input, transition);

            let retirements = flow.capture_retirements(mutation, StorageValueBoundary::Execution);

            if !retirements.is_empty() {
                let mut retire = crate::ExecutionCondition::retiring_storage(retirements);

                for value in values.values_mut() {
                    *value = retire(value);
                }
            }

            Some((completion, values))
        });

        self.apply_published_cleanup(state, mutation);

        if let Some((completion, mut values)) = publication {
            for (destination, snapshot) in completion.publications() {
                let StorageValueDestination::Current { target, ty, .. } = destination else {
                    unreachable!("lifecycle receiver publication changes current storage");
                };

                let Some(value) = values.remove(destination) else { continue; };

                let value = self.complete_captured_references(state, mutation, value)
                    .captured_value(snapshot.clone());

                self.publish_current_value(state, target, *ty, value);
            }
        }

        state.retire_captured_storage(flow.capture_retirements(mutation, StorageValueBoundary::Retirement));
    }

    #[expect(clippy::too_many_arguments, reason = "cancellation uses the checked part and its reached replacement place")]
    pub(super) fn cancel_cleanup(
        &self,
        state: &mut ExecutionState,
        access: StorageAccessId,
        entry: Option<&StorageCleanupEntry>,
        observe: &mut Option<&mut Observer<'_>>,
        point: &AnalysisOperation,
        replacement: Option<ReplacementPath<'_>>,
        mutate: &mut Option<&mut Mutation<'_>>,
    ) {
        let steps = self.disposal.steps.get(&access)
            .expect("selected cancellation has prepared cleanup parts");

        for (part, step) in steps.iter().enumerate().filter(|(_, step)| step.phases().includes_cancellation()) {
            let mutation = StorageMutation::Cleanup(StorageCleanupAction {
                occurrence: point.kind().occurrence(),
                operation: point.id(),
                access,
                part,
                kind: StorageCleanupActionKind::Broadcast,
            });

            if mutate.is_none() && self.published_cleanup().outcome_mutation(mutation).is_none() {
                continue;
            }

            let condition = self.cleanup_part_condition(state, step.selection(), |owner| {
                part_absent(access, part, AsyncCleanupPhases::Cancellation, owner, entry, mutate)
            }, replacement);

            let ty = match step {
                Step::Opaque { ty, .. } | Step::Lifecycle { ty, .. } => *ty,
            };

            let path = step.selection().target();

            let completes = super::selection::select_cleanup(state, condition, mutate, |state, mutate| {
                if let Some(observe) = observe {
                    observe(CleanupObservation::Cancellation);
                }

                if let Some(mutate) = mutate {
                    let route = step.selection().route.as_ref();

                    let action = CleanupMutation::Opaque { ty, access, path, route, failures: &[] };

                    mutate(CleanupTransition::Action(state, mutation, action))
                } else {
                    self.apply_published_cleanup(state, mutation);

                    true
                }
            });

            assert!(completes, "a cancellation broadcast has a continuation");
        }
    }

    #[expect(clippy::too_many_arguments, reason = "cleanup resumes the checked action at its reached replacement place")]
    pub(super) fn dispose(
        &self,
        state: &mut ExecutionState,
        remaining: &[StorageAccessId],
        entry: Option<&StorageCleanupEntry>,
        observe: &mut Option<&mut Observer<'_>>,
        point: &AnalysisOperation,
        after: Option<StorageCleanupAction>,
        replacement: Option<ReplacementPath<'_>>,
        mutate: &mut Option<&mut Mutation<'_>>,
    ) -> bool {
        let access = remaining[0];

        let steps = self.disposal.steps.get(&access)
            .expect("selected cleanup has prepared transitions");

        let (start, after_slot) = match after {
            Some(StorageCleanupAction { access: paused, part, kind: StorageCleanupActionKind::Lifecycle(slot), .. }) => {
                assert_eq!(access, paused, "cleanup resumes its current owner");

                (part, Some(slot))
            }
            Some(StorageCleanupAction { access: paused, part, kind: StorageCleanupActionKind::Opaque, .. }) => {
                assert_eq!(access, paused, "cleanup resumes its current owner");

                (part + 1, None)
            }
            Some(StorageCleanupAction { access: paused, part, kind: StorageCleanupActionKind::RepeatArray, .. }) => {
                assert_eq!(access, paused, "a repeated array resumes its current owner");

                (part, None)
            }
            None => (0, None),
            Some(mutation) => unreachable!("cleanup cannot resume after {mutation:?}"),
        };

        for (part, step) in steps.iter().enumerate().skip(start) {
            let after = (part == start).then_some(after_slot).flatten();

            if step.phases().includes_lifecycle() && after != Some(TypeAssociatedLifecycleSlot::Destructor) {
                let condition = if after == Some(TypeAssociatedLifecycleSlot::Finalizer) {
                    // Reaching the finalizer already selected this part for destruction.
                    crate::ExecutionCondition::boolean(true).into()
                } else {
                    self.cleanup_part_condition(state, step.selection(), |owner| {
                        part_absent(access, part, AsyncCleanupPhases::Lifecycle, owner, entry, mutate)
                    }, replacement)
                };

                if !super::selection::select_cleanup(state, condition, mutate, |state, mutate| {
                    self.dispose_part(state, remaining, part, step, observe, point, (after, replacement), mutate)
                }) {
                    return false;
                }
            }

            if (mutate.is_some() || observe.is_some())
                && let Some(first) = step.selection().repeat_from
                && steps[first..=part].iter().any(|step| step.phases().includes_lifecycle())
            {
                let scope = match point.kind() {
                    AnalysisOperationKind::ScopeExit { scope, exit, .. } => Some(bray_bound_tree::StorageExitPoint::new(scope, exit)),
                    _ => None,
                };

                let action = StorageCleanupAction {
                    occurrence: point.kind().occurrence(), operation: point.id(), access, part: first,
                    kind: StorageCleanupActionKind::RepeatArray,
                };

                let remaining = bray_bound_tree::StorageCleanupRemainder::at(scope, remaining, action);

                if let Some(mutate) = mutate {
                    assert!(mutate(CleanupTransition::Action(state, StorageMutation::Cleanup(action), CleanupMutation::Repeat(remaining))));
                } else if let Some(observe) = observe {
                    let mut continued = state.clone();

                    continued.cleanup = Some(CleanupProgress::repeat(state.cleanup.as_ref(), remaining));

                    observe(CleanupObservation::Continuation(Box::new(continued)));
                }
            }
        }

        true
    }

    #[expect(clippy::too_many_arguments, reason = "one selected cleanup part retains its source, proof and mutation observer")]
    fn dispose_part(
        &self,
        state: &mut ExecutionState,
        remaining: &[StorageAccessId],
        part: usize,
        step: &Step,
        observe: &mut Option<&mut Observer<'_>>,
        point: &AnalysisOperation,
        (after, replacement): (Option<TypeAssociatedLifecycleSlot>, Option<ReplacementPath<'_>>),
        mutate: &mut Option<&mut Mutation<'_>>,
    ) -> bool {
        let operation = point.kind();
        let occurrence = operation.occurrence();
        let scope_exit = matches!(operation, AnalysisOperationKind::ScopeExit { .. });
        let access = remaining[0];
        let solving = mutate.is_some();
        let path = step.selection().target();
        let route = step.selection().route.as_ref();
        let reached = path.and_then(|path| cleanup_path(path, replacement));

        let Step::Lifecycle {
            ty,
            place,
            whole,
            represented,
            ..
        } = step
        else {
            let Step::Opaque { ty, releases_storage, failures, .. } = step else {
                unreachable!("non-lifecycle step is opaque cleanup");
            };

            let action = StorageCleanupAction {
                occurrence,
                access,
                operation: point.id(),
                part,
                kind: StorageCleanupActionKind::Opaque,
            };

            if !solving && !self.cleanup_action_reached(action) {
                return true;
            }

            if scope_exit && let Some(observe) = observe {
                observe(CleanupObservation::Part { access, ty: *ty, releases_storage: *releases_storage, before: state });
            }

            let disposed = (!matches!(
                self.request.semantic_values().type_data(*ty).as_ref(),
                bray_symbols::TypeData::Borrow { .. }
            ))
            .then(|| {
                reached.as_deref().map(|path| self.storage_path_value(state, path))
            })
            .flatten();

            if let Some(mutate) = mutate {
                // Inactive cleanup can retain reports even when its captures
                // leave every ordinary input observation unchanged.
                let effect = if replacement.is_none_or(|(source, reached)| reached == Some(source))
                    && self.cleanup_preserves_inputs(state, access)
                    && crate::representation::type_representation(self.request, *ty)
                        != Some(bray_compiler_known::RepresentationRole::Future)
                {
                    CleanupMutation::Unchanged
                } else {
                    CleanupMutation::Opaque {
                        ty: *ty,
                        access,
                        path,
                        route,
                        failures,
                    }
                };

                if !mutate(CleanupTransition::Action(state, StorageMutation::Cleanup(action), effect)) {
                    return false;
                }
            } else if !self.replay_cleanup_action(state, action, remaining, None, observe) {
                return false;
            }

            if let Some(disposed) = disposed {
                // Destruction ends captured ownership even when cleanup has no user storage effects.
                let place = reached.as_deref().and_then(|path| ExecutionPlace::storage_path(self.storage, path));

                let external = place.iter().flat_map(|place| state.current.entry_borrows_under(place))
                    .collect::<Vec<_>>();

                if let Some(place) = &place {
                    state.current.retire(place);
                }

                let retained = state.current.live_values()
                    .chain(state.pending_results.values().cloned())
                    .chain(external)
                    .collect::<crate::execution_guarantees::RetainedSnapshots>();

                state.retire_trusted_values([disposed], retained);
            }

            return true;
        };

        let completed = match after {
            None => 0,
            Some(TypeAssociatedLifecycleSlot::Finalizer) => 1,
            Some(slot) => unreachable!("cleanup cannot resume after {slot:?}"),
        };

        let mut slots = [
            TypeAssociatedLifecycleSlot::Finalizer,
            TypeAssociatedLifecycleSlot::Destructor,
        ].into_iter().skip(completed).filter_map(|slot| {
            let lifecycle = self.disposal.lifecycles.get(&(*ty, slot))?;

            let action = StorageCleanupAction {
                occurrence,
                operation: point.id(),
                access,
                part,
                kind: StorageCleanupActionKind::Lifecycle(slot),
            };

            (solving || self.cleanup_action_reached(action)).then_some((slot, lifecycle, action))
        }).peekable();

        if scope_exit && !*represented && let Some(observe) = observe
            && slots.peek().is_some()
        {
            observe(CleanupObservation::Part { access, ty: *ty, releases_storage: false, before: state });
        }

        let place = if replacement.is_some() {
            reached.as_deref().and_then(|path| ExecutionPlace::storage_path(self.storage, path))
        } else {
            place.clone()
        };

        for (slot, lifecycle, action) in slots {
            let value = reached.as_deref().map(|path| self.storage_path_value(state, path));

            let mut evidence = lifecycle.entry(state, value);

            self.preserve_integer_entry_conditions(
                state,
                &lifecycle.contract.preconditions,
                &mut evidence,
            );

            let requirements = self.prove_trusted_requirements(&evidence, &lifecycle.contract.requirements)
                .and_then(|()| evidence.prove_requirements_with_cache(&self.proofs, lifecycle.contract.preconditions.iter()
                    .map(|(condition, _)| condition)));

            let established = requirements.is_ok();

            let template = &self.dependencies.lifecycle_dependencies()[&lifecycle.call.callable];

            evidence.failure_rows = self.select_frame_failures(&evidence, template.failure_outcomes());

            let evidence = Arc::new(evidence);

            if let Some(observe) = observe {
                observe(CleanupObservation::Invocation {
                    access,
                    part,
                    slot,
                    lifecycle,
                    whole: *whole,
                    represented: *represented,
                    evidence: &evidence,
                    proof: &requirements,
                });
            }

            if let Some(mutate) = mutate {
                let outcomes = template.captured_values().iter().enumerate().filter_map(|(index, row)| {
                    self.cleanup_normal_guard(state, &evidence, row).map(|guard| (index, row, guard))
                })
                    .collect();

                let invocation = CleanupMutation::Lifecycle(LifecycleInvocation {
                        ty: *ty,
                        access,
                        lifecycle,
                        entry: &evidence,
                        contract: template,
                        completes: established,
                        path,
                        route,
                        outcomes,
                    });

                if !mutate(CleanupTransition::Action(state, StorageMutation::Cleanup(action), invocation)) {
                    return false;
                }
            } else if !self.replay_cleanup_action(state, action, remaining, Some((&evidence, template)), observe) {
                return false;
            }

            if established {
                self.complete_trusted_cleanup(state, place.clone(), lifecycle, &evidence);
            }
        }

        true
    }
}

impl Lifecycle {
    pub(in crate::analysis) fn is_infallible_destructor(
        &self,
        dependency: &bray_symbols::DependencyContractTemplateData,
    ) -> bool {
        self.call.result.execution() == bray_symbols::CallableExecution::Synchronous
            && self.contract.requirements.is_empty()
            && self.contract.execution_requirements.is_empty()
            && self.contract.preconditions.is_empty()
            && dependency.normal_completion()
            && dependency.failure_outcomes().is_some_and(<[_]>::is_empty)
    }

    fn entry(
        &self,
        state: &ExecutionState,
        value: Option<crate::ExecutionCondition>,
    ) -> ExecutionCallEvidence {
        // Capture one actual receiver snapshot. Its components are projections
        // of this value, including when the receiver itself is a component.
        let value = value.unwrap_or_else(crate::ExecutionCondition::unknown);

        let receiver = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0));

        let addresses = if self.contract.address_subjects.is_empty() {
            Default::default()
        } else {
            [(receiver.clone(), state.address_dependencies(&value))].into()
        };

        let arguments = BTreeMap::from([
            (self.receiver.into(), value.clone()),
            (receiver, value),
        ]);

        let assumptions = state.assumptions.clone();

        // Entry assumptions must survive the mutations made by this lifecycle invocation.
        ExecutionCallEvidence {
            failure_rows: None,
            outputs: Default::default(),
            implementation_witnesses: Default::default(),
            witness_inputs: Default::default(),
            witness_allocations: Default::default(),
            extracted_values: Default::default(),
            owned_inputs: Default::default(),
            borrow_owners: Default::default(),
            addresses,
            anonymous_target: None,
            trusted_boundary: !state.trust_boundaries.is_empty(),
            pending_execution: false,
            storage_unchanged: false,
            memory_access: None,
            arguments,
            assumptions,
            trusted_assumptions: state.trusted_assumptions.clone(),
        }
    }
}
