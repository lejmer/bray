use bray_bound_tree::{StorageValueDestination, StorageValueInput};
use bray_symbols::CapturedValueTemplate as Value;

use super::model::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionPlace};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn invocation_value_transitions(&self, state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite, bindings: &[bray_symbols::CapturedValueOutcome],
    ) -> Vec<bray_bound_tree::StorageValueTransition> {
        let selection = invocation.expression().and_then(|expression| self.semantics.selections().expression(expression));

        self.entry_value_transitions(state.entries.get(&invocation), bindings,
            |root| selection?.invocation_input_ordinal(invocation, root))
    }

    pub(super) fn entry_value_transitions(&self, entry: Option<&crate::ExecutionCallEvidence>,
        bindings: &[bray_symbols::CapturedValueOutcome],
        ordinal: impl Fn(bray_symbols::DependencySubjectRoot) -> Option<bray_symbols::SymbolOrdinal>,
    ) -> Vec<bray_bound_tree::StorageValueTransition> {
        bindings.iter().map(|outcome| {
            let mut writes = Vec::new();

            for (subject, template) in outcome.bindings.iter() {
                let value = self.storage_value_expression(template);

                if subject.subject_root() == bray_symbols::DependencySubjectRoot::Result {
                    writes.push((subject, StorageValueDestination::Result, value));

                    continue;
                }

                let descriptor = entry.and_then(|entry| {
                    self.entry_subject(entry, ordinal(subject.subject_root())?, subject)
                });

                let Some(storage) = descriptor.as_ref().and_then(ExecutionCondition::inline_captured_storage) else {
                    continue;
                };

                let Some(ty) = storage.observations().find_map(|(path, kind)| {
                    let bray_bound_tree::StorageObservationKind::Reference(reference) = kind else { return None; };

                    if !path.is_empty() { return None; }

                    match *self.request.semantic_values().type_data(reference) {
                        bray_symbols::TypeData::Borrow { target, .. } => Some(target),
                        _ => None,
                    }
                }) else { continue; };

                if let Some(target) = storage.known_address() {
                    writes.push((subject, StorageValueDestination::Current { target: target.clone(), ty }, value));
                } else if let Some(regions) = storage.address_regions() {
                    writes.extend(regions.into_iter().map(|target| (
                        subject,
                        StorageValueDestination::Region { target, via: subject.clone() },
                        value.clone(),
                    )));
                }
            }

            // The exit describes writes in callee coordinates. Instantiation can make
            // independent formal destinations overlap, losing their exact final value.
            let conflicted = writes.iter().enumerate().filter_map(|(index, (_, destination, value))| {
                let StorageValueDestination::Current { target, .. } = destination else { return None; };

                writes.iter().any(|(_, other, incoming)| {
                    let reached = match other {
                        StorageValueDestination::Result => return false,
                        StorageValueDestination::Current { target, .. } if other != destination || incoming != value => target,
                        StorageValueDestination::Current { .. } => return false,
                        StorageValueDestination::Region { target, .. } => target,
                    };

                    target.relationship_in_slots(reached, self.storage.identities()) != bray_bound_tree::StorageRelationship::Disjoint
                }).then_some(index)
            }).collect::<std::collections::BTreeSet<_>>();

            let writes = writes.into_iter().enumerate().map(|(index, (subject, destination, value))| {
                let destination = match destination {
                    StorageValueDestination::Current { target, .. } if conflicted.contains(&index) => {
                        StorageValueDestination::Region { target, via: subject.clone() }
                    }
                    destination => destination,
                };

                (destination, value)
            });

            bray_bound_tree::StorageValueTransition::new(outcome.guard, writes)
                .with_ownership(outcome.ownership.clone())
                .with_carriers(self.bound_carriers(&outcome.carriers).iter().cloned())
        }).collect()
    }

    pub(in crate::analysis::guarantee) fn bound_carriers(&self, carriers: &[bray_symbols::DependencyWitnessCarrier])
        -> std::sync::Arc<[bray_symbols::DependencyWitnessCarrier<bray_bound_tree::StorageProjection>]> {
        carriers.iter().map(|carrier| {
            let path = crate::storage::storage_projections(self.request.semantic_values(), self.dependencies, carrier.path())
                .expect("checked result carrier retains its selected projections");

            carrier.remap(carrier.guarantee(), path)
        }).collect()
    }

    pub(in crate::analysis) fn entry_subject(&self, entry: &crate::ExecutionCallEvidence,
        ordinal: bray_symbols::SymbolOrdinal, subject: &bray_symbols::DependencySubject,
    ) -> Option<ExecutionCondition> {
        let value = ExecutionPlace::argument(ordinal).value_in(&entry.arguments)?;
        let path = crate::storage::storage_projections(self.request.semantic_values(), self.dependencies, subject.projections())?;

        Some(path.into_iter().fold(value, ExecutionCondition::project))
    }

    fn storage_value_expression(&self, value: &Value) -> Value<StorageValueInput, bray_bound_tree::StorageProjection> {
        value.try_map(&mut |term| Ok::<_, ()>(StorageValueInput::Term(*term)), &mut |projection| {
            crate::storage::storage_projections(self.request.semantic_values(), self.dependencies, &[*projection])
                .and_then(|values| values.into_iter().next()).ok_or(())
        }, &mut |root| Ok(root.clone()), &mut |callee| Ok(callee.clone()), &mut |variant| Ok(*variant)).unwrap_or(Value::Unknown)
    }

    fn captured_destination(&self, invocation: bray_bound_tree::BoundExecutionSite,
        destination: &StorageValueDestination,
    ) -> Option<(ExecutionPlace, bool)> {
        match destination {
            StorageValueDestination::Region { .. } => None,
            StorageValueDestination::Result => self.invocation_result_place(invocation).map(|place| (place, false)),
            StorageValueDestination::Current { target, ty } => Some(self.current_target_place(target, *ty)),
        }
    }

    /// The completed current value, independently of its prebound private prediction.
    pub(in crate::analysis) fn captured_output(&self, state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite, destination: &StorageValueDestination,
    ) -> Option<ExecutionCondition> {
        match destination {
            StorageValueDestination::Region { .. } => None,
            StorageValueDestination::Result => Some(self.invocation_value(state, invocation)),
            StorageValueDestination::Current { target, ty } => self.current_target_value(state, target, Some(*ty)),
        }
    }

    /// Publishes one paired value to current storage and its admitted outcome.
    pub(in crate::analysis) fn publish_captured_output(&self, state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite, destination: &StorageValueDestination, value: ExecutionCondition,
    ) {
        if *destination == StorageValueDestination::Result
            && let bray_bound_tree::BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) = invocation
        {
            self.publish_expression(state, expression, value.clone());
        } else if let Some((place, borrowed)) = self.captured_destination(invocation, destination) {
            let stored = match place.value_in(&state.current) {
                Some(previous) if borrowed && previous.is_reference() => previous.with_referent(value.clone()),
                _ => value.clone(),
            };

            state.assign(place, stored, None);
        }

        if let Some(entry) = state.entries.get_mut(&invocation) {
            // The destination and entry retain handles to the same immutable evaluated value.
            entry.outputs.publish(destination.clone(), value);
        }
    }

    pub(in crate::analysis::guarantee) fn replay_captured_publications(&self, state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite, completion: &bray_bound_tree::StorageCompletion,
    ) {
        for (destination, snapshot) in completion.publications() {
            let Some(value) = self.captured_output(state, invocation, destination) else { continue; };

            self.publish_captured_output(state, invocation, destination, value.captured_value(snapshot.as_ref().clone()));
        }
    }

    pub(in crate::analysis) fn capture_entry_values(&self, state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite, captures: &[bray_bound_tree::StorageValueCapture],
    ) {
        let Some(mut entry) = state.entries.remove(&invocation) else { return; };

        for capture in captures {
            let input = ExecutionPlace::argument(capture.input);

            let Some(mut value) = input.value_in(&entry.arguments) else { continue; };

            if value.is_reference() {
                let referent = capture.referent.as_ref().and_then(|(ty, snapshot)| {
                    let target = capture.value.known_address()?;

                    self.current_target_value(state, target, Some(*ty))
                        .map(|value| value.captured_value(snapshot.as_ref().clone()))
                });

                // The admitted logical value already observes this incoming state.
                // Missing physical layout knowledge cannot erase that established data.
                if let Some(referent) = referent { value = value.with_referent(referent); }
            }

            if let Some((pointer, bounds)) = &capture.element_bounds {
                let attach = |value: ExecutionCondition| {
                    let prefix = ExecutionCondition::storage_value(bray_symbols::CapturedValueOperation::Select(bray_bound_tree::StorageProjection::OwnedTarget), vec![value.clone()].into());

                    if matches!(prefix.computed_value().kind(), crate::ExecutionConditionKind::StorageValue(bray_symbols::CapturedValueOperation::OwningStorage(_), _)) {
                        return value;
                    }

                    let prefix = prefix.captured_value(bounds.as_ref().clone());

                    value.with_storage_component(&[bray_bound_tree::StorageProjection::OwnedTarget], ExecutionCondition::storage_value(
                        bray_symbols::CapturedValueOperation::OwningStorage(*pointer), vec![prefix].into()))
                };

                value = if value.is_reference() {
                    let contents = value.clone().into_referent();

                    value.with_referent(attach(contents))
                } else { attach(value) };
            }

            if value.inline_captured_storage().is_none() {
                value = value.captured_value(capture.value.as_ref().clone());
            }

            entry.arguments.insert(input, value);
        }

        state.entries.insert(invocation, entry);
    }

    pub(in crate::analysis) fn prepare_captured_values(
        &self, state: &mut ExecutionState, invocation: bray_bound_tree::BoundExecutionSite,
        transitions: &[bray_bound_tree::StorageValueTransition],
        portable_order: bool,
    ) {
        if transitions.is_empty() { return; }

        let producer = invocation.expression().map(|expression|
            self.observed_call_result(state, expression).unwrap_or_else(||
                ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression))))
            .unwrap_or_else(ExecutionCondition::unknown);

        let Some(mut entry) = state.entries.remove(&invocation) else { return; };

        let input_count = self.dependencies.calls().get(&invocation).map(|call| call.input_count())
            .or_else(|| invocation.expression().and_then(|expression| self.semantics.selections().expression(expression))
                .map(|selection| selection.inputs().count()))
            .expect("captured transition has a checked receiver-first input environment");

        entry.outputs = self.bind_entry_outputs(state, &entry, invocation, input_count, producer, transitions, portable_order);
        state.entries.insert(invocation, entry);
    }

    pub(super) fn bind_entry_outputs(&self, state: &ExecutionState, entry: &crate::ExecutionCallEvidence,
        invocation: bray_bound_tree::BoundExecutionSite, input_count: usize, producer: ExecutionCondition,
        transitions: &[bray_bound_tree::StorageValueTransition],
        portable_order: bool,
    ) -> crate::execution_guarantees::ExecutionOutputs {
        let decode = |term| crate::execution_condition_from_type_term(self.request.semantic_values(), term, input_count)
            .expect("checked captured term belongs to its call environment");

        let term = |term| entry.bind_condition(&decode(term), &producer);

        let input = |input: &StorageValueInput| match input {
            StorageValueInput::Argument(ordinal) => ExecutionPlace::argument(*ordinal).value_in(&entry.arguments).unwrap_or_else(ExecutionCondition::unknown),
            StorageValueInput::Term(source) => term(*source),
        };

        let mut outcomes = Vec::new();

        let mut alternative = 0;

        for (index, transition) in transitions.iter().enumerate() {
            use crate::execution_guarantees::OutputGuard;

            let sources = portable_order.then(|| [index].into());

            let guard = match transition.guard.map(&decode) {
                Some(formal) if formal.observes_completion_result() => OutputGuard::Completion(formal),
                Some(formal) => OutputGuard::Bound(entry.bind_condition(&formal, &producer)),
                None => OutputGuard::Bound(ExecutionCondition::boolean(true)),
            };

            // A compound exit guard may already fail its input conditions even
            // while its result condition awaits private production.
            let admission = guard.bind(entry, &ExecutionCondition::unknown());

            if !state.proofs.ordinary_proof(&entry.assumptions).can_assume(&admission, true) {
                alternative += transition.bindings.iter().flat_map(|(_, value)| value.nodes()).map(|node| match node {
                    Value::Deferred { alternatives, .. } => alternatives.len(), _ => 0,
                }).sum::<usize>();

                outcomes.push(crate::execution_guarantees::ExecutionOutput { sources, guard: OutputGuard::Bound(ExecutionCondition::boolean(false)), carriers: Default::default(), ownership: Default::default(), writes: Default::default() });
                continue;
            }

            let writes = transition.bindings.iter().map(|(destination, value)| {
            let value = self.bind_captured_value(state, entry, value, &input, bray_bound_tree::StorageMutation::Operation(invocation), &mut alternative);

            let value = match destination {
                StorageValueDestination::Region { .. } => value,
                StorageValueDestination::Result => value.with_producer(producer.clone()),
                StorageValueDestination::Current { .. } => {
                    let (place, _) = self.captured_destination(invocation, destination)
                        .expect("a resolved current publication retains its destination");

                    let contents = ExecutionCondition::post_state(bray_bound_tree::StorageMutation::Operation(invocation),
                        ExecutionCondition::new(crate::ExecutionConditionKind::Input(place)));

                    value.copied_value().clone().with_producer(contents)
                }
            };

            (destination.clone(), value)
            }).collect::<std::collections::BTreeMap<_, _>>();

            outcomes.push(crate::execution_guarantees::ExecutionOutput { sources, guard, carriers: std::sync::Arc::clone(&transition.carriers), ownership: transition.ownership.clone(), writes });
        }

        crate::execution_guarantees::ExecutionOutputs(outcomes.into())
    }

    pub(in crate::analysis::guarantee) fn complete_entry_outputs(&self, state: &mut ExecutionState,
        admission: Option<bray_bound_tree::BoundExecutionSite>, publication: bray_bound_tree::BoundExecutionSite,
        result: ExecutionCondition,
        entry: &crate::ExecutionCallEvidence,
        mut finish: impl FnMut(&mut ExecutionState, ExecutionCondition, &bray_symbols::DependencyOwnedResult, &[bray_symbols::DependencyWitnessCarrier<bray_bound_tree::StorageProjection>]) -> ExecutionCondition,
    ) -> ExecutionCondition {
        let outputs = &entry.outputs;

        if outputs.0.is_empty() {
            return finish(state, result, &Default::default(), &[]);
        }

        let mut feasible = outputs.0.iter().filter_map(|row| {
            let guard = row.guard.bind(entry, &result);

            self.admits_output(&state.assumptions, entry, &result, &guard, &row.ownership).then_some((row, guard))
        });

        let Some(first) = feasible.next() else { return result; };

        let mut complete = |branch: &mut ExecutionState, (row, guard): (&crate::execution_guarantees::ExecutionOutput, ExecutionCondition)| {
            let value = self.complete_output_row(branch, admission, publication, result.clone(), row, guard);

            finish(branch, value, &row.ownership, &row.carriers)
        };

        let Some(second) = feasible.next() else { return complete(state, first); };

        let expression = publication.expression().expect("normal call outcomes retain their publishing expression");
        let mut completed = state.clone();
        let mut value = complete(&mut completed, first);

        for row in std::iter::once(second).chain(feasible) {
            let mut branch = state.clone();
            let incoming = complete(&mut branch, row);

            value = super::merge_returned(&mut completed, &branch, &value, &incoming, &result, expression);
        }

        *state = completed;

        value
    }

    fn complete_output_row(&self, state: &mut ExecutionState,
        admission: Option<bray_bound_tree::BoundExecutionSite>, publication: bray_bound_tree::BoundExecutionSite,
        mut result: ExecutionCondition, row: &crate::execution_guarantees::ExecutionOutput, guard: ExecutionCondition,
    ) -> ExecutionCondition {
        if let Some(entry) = admission.and_then(|admission| state.entries.get_mut(&admission)) {
            entry.outputs = crate::execution_guarantees::ExecutionOutputs([crate::execution_guarantees::ExecutionOutput {
                sources: None,
                guard: crate::execution_guarantees::OutputGuard::Bound(guard.clone()), ..row.clone()
            }].into());
        }

        guard.assume(true, &mut state.assumptions);

        for (destination, value) in &row.writes {
            let value = value.clone();
            let value = self.complete_captured_references(state, publication, value);

            if *destination == StorageValueDestination::Result { result = value.with_producer(result); }
            else { self.publish_captured_output(state, publication, destination, value); }
        }

        result
    }

    fn bind_captured_value(
        &self,
        state: &ExecutionState,
        entry: &crate::ExecutionCallEvidence,
        value: &Value<StorageValueInput, bray_bound_tree::StorageProjection>,
        input: &impl Fn(&StorageValueInput) -> ExecutionCondition,
        mutation: bray_bound_tree::StorageMutation,
        alternative: &mut usize,
    ) -> ExecutionCondition {
        match value {
            Value::Input(source) => input(source),
            Value::Static(selection) => ExecutionCondition::new(crate::ExecutionConditionKind::Input(
                ExecutionPlace::static_instance(selection.clone()))),
            Value::Data(source) => {
                let value = input(source);

                ExecutionCondition::scalar_observation(value.owner_identity().clone(), value.computed_value().clone())
            }
            Value::Deferred { alternatives, unknown } => {
                let mut frame = if *unknown {
                    crate::ExecutionDeferredValue::unknown(&state.assumptions)
                } else { crate::ExecutionDeferredValue::default() };

                for invocation in alternatives.iter() {
                    let callee = invocation.call.clone();

                    let key = crate::execution_guarantees::DeferredProducer::Summary {
                        instantiation: mutation, callee: callee.clone(), alternative: *alternative,
                    };

                    *alternative += 1;

                    let arguments = invocation.captures.iter().enumerate().map(|(index, value)| {
                        let ordinal = bray_symbols::SymbolOrdinal::new(u32::try_from(index).expect("checked capture ordinal fits"));

                        (ExecutionPlace::argument(ordinal), self.bind_captured_value(state, entry, value, input, mutation, alternative))
                    }).collect();

                    let mut selection = super::super::deferred::selection(&state.assumptions);

                    if let Some(guard) = &invocation.guard {
                        input(guard).assume(true, &mut selection);
                    }

                    let mut captured = crate::ExecutionCallEvidence {
                        arguments,
                        owned_inputs: std::sync::Arc::clone(&invocation.owned_inputs),
                        pending_execution: true,
                        ..Default::default()
                    };

                    state.transfer_frame_admission(entry, &mut captured);

                    frame.producers.insert(key, crate::ExecutionDeferredInvocation {
                        callee, transition: None, selection, completed: None, reports: Default::default(),
                        entry: std::sync::Arc::new(captured),
                    });
                }

                ExecutionCondition::unknown().with_deferred_value(std::sync::Arc::new(frame))
            }
            Value::Unknown => ExecutionCondition::unknown(),
            Value::Tagged(variant, value) => ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(*variant,
                std::sync::Arc::new(self.bind_captured_value(state, entry, value, input, mutation, alternative)))),
            Value::Operation(bray_symbols::CapturedValueOperation::Read, values) => {
                let reference = self.bind_captured_value(state, entry, &values[0], input, mutation, alternative);
                let reference = self.refresh_captured_references(state, reference, None);

                ExecutionCondition::storage_value(bray_symbols::CapturedValueOperation::Read, vec![reference].into())
            }
            Value::Operation(bray_symbols::CapturedValueOperation::Inline, values) =>
                super::preservation::ValuePreservation::inline_snapshot(&self.bind_captured_value(state, entry, &values[0], input, mutation, alternative), mutation),
            Value::Operation(operation, values) => ExecutionCondition::storage_value(*operation,
                values.iter().map(|value| self.bind_captured_value(state, entry, value, input, mutation, alternative)).collect()),
            Value::Aggregate { base, fields } => ExecutionCondition::constructed(
                self.bind_captured_value(state, entry, base, input, mutation, alternative), fields.iter().map(|(field, value)|
                    (*field, self.bind_captured_value(state, entry, value, input, mutation, alternative))).collect()),
        }
    }

    /// Refreshing a reached view does not change the actual enclosing owner's lineage.
    pub(in crate::analysis::guarantee) fn complete_captured_references(&self, state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite, value: ExecutionCondition,
    ) -> ExecutionCondition {
        let ownership = state.owned_result(&value);
        let refreshed = self.refresh_captured_references(state, value.clone(), Some(bray_bound_tree::StorageMutation::Operation(invocation)));

        if refreshed != value { state.owned_results.insert(refreshed.clone(), ownership); }

        refreshed
    }

    /// A captured descriptor survives its old slot. Its reached data observes current memory.
    pub(in crate::analysis::guarantee) fn refresh_captured_references(&self, state: &ExecutionState, value: ExecutionCondition,
        completed: Option<bray_bound_tree::StorageMutation>,
    ) -> ExecutionCondition {
        let mut refreshed = value.clone();

        for (path, kind) in value.captured_observations() {
            let bray_bound_tree::StorageObservationKind::Reference(ty) = kind else { continue; };

            let bray_symbols::TypeData::Borrow { target: ty, .. } = *self.request.semantic_values().type_data(ty) else { continue; };

            let descriptor = path.iter().copied().fold(value.clone(), ExecutionCondition::project);

            if !descriptor.is_reference() { continue; }

            let Some(storage) = descriptor.inline_captured_storage() else { continue; };

            let reached = storage.known_address().and_then(|target| self.current_target_value(state, target, Some(ty)));

            let reached = match (reached, completed) {
                (Some(reached), _) => reached,
                (None, Some(mutation)) => ExecutionCondition::post_state(mutation, descriptor.owner_identity().clone().into_referent()),
                (None, None) => continue,
            };

            refreshed = refreshed.with_storage_component(&path, descriptor.with_referent(reached));
        }

        refreshed
    }
}
