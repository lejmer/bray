use bray_bound_tree::BoundExecutionSite;
use bray_symbols::{DependencyMutationKind as Kind, DependencySubjectRoot};

use super::core::StorageFlowCollector;
use crate::analysis::storage_flow::memory::{Effect, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::CheckerRequestContext;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn start_frame_effects(
        &self, memory: &crate::analysis::storage_flow::memory::State,
        execution: &mut crate::analysis::guarantee::ExecutionState, invocation: BoundExecutionSite,
    ) {
        let BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) = invocation else {
            return;
        };

        let value = match self.request.view().expression(expression) {
            Some(bray_bound_tree::BoundExpression::Await(_)) => execution.admitted_receiver(invocation),
            _ => {
                let Some(value) = self.execution.started_future(execution, expression) else { return; };

                value
            }
        };

        let Some(frame) = value.deferred_value() else { return; };

        let mut started = std::sync::Arc::clone(frame);
        let proof = execution.ordinary_proof();

        for (source, candidate) in &frame.producers {
            if !crate::execution_guarantees::deferred_possible(&candidate.selection, &proof) { continue; }

            let effects = if self.execution.is_task_observation(source.clone()) {
                crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)).value_in(&candidate.entry.arguments).and_then(|task| {
                    let task = task.deferred_value()?;

                    super::effects::task_effects(task.effects(&proof), task.cleanup_effects(false, &proof)?)
                })
            } else {
                self.bind_frame_effects(memory, source, candidate)
            };

            if candidate.transition.as_deref().and_then(|transition| transition.transfer.deferred_effects.as_ref()) == effects.as_ref() {
                continue;
            }

            let candidate = std::sync::Arc::make_mut(&mut started).producers.get_mut(source)
                .expect("execution starts the selected frame");

            let transition = candidate.transition.get_or_insert_with(|| std::sync::Arc::new(
                bray_bound_tree::StorageDeferredTransition { output: None, transfer: Default::default() }
            ));

            std::sync::Arc::make_mut(transition).transfer.deferred_effects = effects;
        }

        if !std::sync::Arc::ptr_eq(&started, frame) {
            execution.replace_deferred_value(frame, Some(started));
        }
    }

    fn bind_frame_effects(
        &self, memory: &crate::analysis::storage_flow::memory::State,
        source: &crate::execution_guarantees::DeferredProducer,
        candidate: &crate::execution_guarantees::ExecutionDeferredInvocation,
    ) -> Option<crate::analysis::storage_flow::memory::EffectOutcomes> {
        let summary;

        let (templates, routes) = if let Some(site) = source.local() {
            let call = self.dependencies.calls().get(&site)?;

            (call.deferred_effect_outcomes()?, call.storage())
        } else {
            let callee = candidate.callee.as_deref()?;
            let body = self.dependencies.frame_callees().get(callee)?;

            summary = self.request.semantic_values().dependency_contract_template_data(body.summary);

            (summary.effect_outcomes(), &body.storage)
        };

        let values = self.request.semantic_values();
        let read = |slot| crate::analysis::storage_flow::memory::Source::Value(slot).mutation_targets(memory);

        let effects = templates.try_flat_map(|effect| {
            Ok::<_, std::convert::Infallible>(self.resolve_effects(std::slice::from_ref(effect), read, |subject, indirect| {
                let root = subject.subject_root();
                let (_, path) = routes.subject(subject)?;

                if root.is_static() {
                    let start = self.static_location(&bray_symbols::DependencySubject::root(root))?;
                    let locations = memory.reference_locations(values, start, path, None)?;

                    return bray_bound_tree::StorageMutationTarget::resolve_locations(locations, indirect, read);
                }

                let ordinal = root.input_ordinal(candidate.has_receiver())?;
                let captured = candidate.entry.argument_storage(ordinal)?;

                memory.captured_effect_targets(values, captured, path, indirect)
            }))
        }).expect("unresolved frame effects retain conservative footprints");

        Some(effects)
    }

    pub(super) fn capture_deferred_cleanup(
        &self,
        state: &mut StorageFlowState,
        execution: &crate::analysis::guarantee::ExecutionState,
        invocation: BoundExecutionSite,
    ) {
        let contracts = &self.dependencies.calls()[&invocation];

        let Some(captures) = contracts.deferred_capture_cleanup() else { return; };

        let result = contracts.deferred_result_cleanup().expect("prepared deferred cleanup retains both phases");
        let roots = self.invocation_roots(state, invocation);
        let output = Target::storage(self.invocation_result(invocation).expect("lazy call retains its completion coordinates"));
        let mut private = vec![output.clone()];

        if let Some(selection) = invocation.expression().and_then(|expression| self.execution.semantics.selections().expression(expression)) {
            private.extend(selection.inputs().filter_map(|input| self.invocation_place(invocation, input.root()).map(Target::storage)));
        }

        let captures = self.capture_opaque_effects(state, invocation, self.call_effects(state, execution, invocation, captures), &roots, &private, &output);

        let read_result = |slot: Target| {
            if output.contains_storage(&slot) {
                state.memory.staged_mutation_targets(invocation, &output, &slot)
            } else {
                crate::analysis::storage_flow::memory::Source::Value(slot).mutation_targets(&state.memory)
            }
        };

        let result = self.resolve_effects(result, read_result, |subject, indirect| {
            let locations = match subject.subject_root() {
                root if root.is_static() => self.invocation_locations(state, invocation, subject)?,
                DependencySubjectRoot::Result => self.invocation_stored_locations(
                    state, invocation, subject, contracts.storage().subject(subject)?.1, true)?,
                _ => return None,
            };

            bray_bound_tree::StorageMutationTarget::resolve_locations(locations, indirect, read_result)
        });

        let result = self.capture_opaque_effects(state, invocation, result, &roots, &private, &output);

        state.memory.capture_deferred_cleanup(invocation, captures, result);
    }

    fn capture_opaque_effects(
        &self,
        state: &StorageFlowState,
        invocation: BoundExecutionSite,
        effects: Vec<Effect>,
        captures: &[Target],
        private: &[Target],
        output: &Target,
    ) -> Vec<Effect> {
        let mut captured = Vec::new();

        for effect in effects {
            match effect {
                Effect::Mutation { footprint: target, kind: Kind::Dispose(ty), .. } => {
                    let (mut regions, unknown) = if output.contains_storage(&target) {
                        state.memory.staged_contained_regions(invocation, &target)
                    } else {
                        let mut regions = std::collections::BTreeSet::new();
                        let mut unknown = false;

                        for (_, targets, opaque) in state.memory.observed_references().filter(|(slot, _, _)| target.contains_storage(slot)) {
                            regions.extend(targets.iter().cloned());
                            unknown |= opaque;
                        }

                        (regions, unknown)
                    };

                    let reference_free = self.input.footprint.complete_reference_types.get(&ty) == Some(&true);

                    if unknown || regions.is_empty() && !reference_free {
                        regions.extend(captures.iter().cloned());
                    }

                    if !private.iter().any(|root| root.contains_storage(&target)) {
                        regions.insert(target);
                    }

                    // Region bounds identify captured referents. They do not claim
                    // that each referent has the disposed value's type or layout.
                    captured.extend(regions.into_iter().map(|region| Effect::bounded(region, Kind::Cleanup(ty), None)));
                    captured.push(Effect::UnboundedMutation { kind: Kind::Dispose(ty), access_type: None, ambient: true });
                }
                Effect::Unknown | Effect::Escape => {
                    let kind = if effect == Effect::Escape { Kind::Escape } else { Kind::Write };

                    captured.extend(captures.iter().cloned().map(|target| Effect::bounded(target, kind, None)));
                    captured.push(Effect::Ambient);
                }
                effect => captured.push(effect),
            }
        }

        captured
    }
}
