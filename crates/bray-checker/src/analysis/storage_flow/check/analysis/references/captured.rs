use bray_bound_tree::{BoundExecutionSite, StorageValueDestination, StorageValueTransition};
use bray_symbols::TypeData;

use super::super::core::StorageFlowCollector;
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::CheckerRequestContext;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn record_current_write(&self, state: &mut StorageFlowState, target: &bray_bound_tree::StorageTarget) {
        if target.is_exact()
            && matches!(target.origin, bray_bound_tree::StorageOrigin::Entry { layer: 1, kind: bray_symbols::DependencyReferenceKind::Borrow, .. })
            && let Some(path) = target.inline_state_path()
            && let Some(input) = self.formal_root(path.root)
        {
            state.written_inputs.insert(input);
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn preserve_captured_observations(&self,
        execution: &crate::analysis::guarantee::ExecutionState, invocation: BoundExecutionSite,
        invalidation: &mut bray_bound_tree::StorageInvalidation,
    ) {
        let snapshots = match invalidation {
            bray_bound_tree::StorageInvalidation::All { normal_snapshots, .. }
            | bray_bound_tree::StorageInvalidation::Observed { normal_snapshots, .. } => normal_snapshots,
            bray_bound_tree::StorageInvalidation::Paths(_) => unreachable!("captured outputs retain resolved observations"),
        };

        let Some(outputs) = execution.admitted_call(invocation).map(|entry| &entry.outputs) else { return; };

        for destination in outputs.destinations() {
            let Some(value) = outputs.common_value(destination) else { continue; };

            for (path, kind) in value.captured_observations() {
                if kind.is_immutable_inline() || kind == bray_bound_tree::StorageObservationKind::Discriminant {
                    snapshots.insert((bray_bound_tree::StorageSnapshotRoot::Produced(destination.clone()), path.into()), kind);
                }
            }
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn publish_captured_outputs(&self,
        state: &mut StorageFlowState, invocation: BoundExecutionSite, completion: &mut bray_bound_tree::StorageCompletion,
    ) {
        let Some(execution) = state.execution.as_ref() else { return; };

        // Completed values and admitted outputs share immutable handles. The plan owns destinations.
        let Some(outputs) = execution.admitted_call(invocation).map(|entry| &entry.outputs) else { return; };

        let regions = outputs.0.iter().flat_map(|row| &row.writes).filter_map(|(destination, value)| match destination {
            StorageValueDestination::Region(target) => Some((target.clone(), value.clone())),
            _ => None,
        }).collect::<std::collections::BTreeSet<_>>();

        let outputs = outputs.destinations().filter_map(|destination|
            self.execution.captured_output(execution, invocation, destination).map(|value| (destination.clone(), value))).collect::<Vec<_>>();

        for (region, value) in regions {
            if let Some(bounds) = value.inline_captured_storage() {
                state.memory.include_captured_bounds(&region, bounds);
            } else {
                state.memory.lose_captured_information(&region);
            }
        }

        for (destination, value) in outputs {
            let target = match &destination {
                StorageValueDestination::Region(_) => unreachable!("a possible region has no current value"),
                StorageValueDestination::Result => {
                    let Some(path) = self.invocation_result(invocation) else { continue; };

                    bray_bound_tree::StorageTarget::storage(path)
                }
                StorageValueDestination::Current { target, .. } => {
                    self.record_current_write(state, target);

                    target.clone()
                }
            };

            let mut snapshot = value.inline_captured_storage().cloned().unwrap_or_else(|| {
                let mut snapshot = state.memory.capture_reached_value(&target);

                if !value.is_reference()
                    && let crate::ExecutionConditionKind::Constructed(_, fields) = value.computed_value().kind()
                {
                    for (field, child) in fields.iter() {
                        if let Some(storage) = child.inline_captured_storage() {
                            snapshot = snapshot.with_component(&[*field], Some(storage));
                        }
                    }
                }

                snapshot
            });

            if destination == StorageValueDestination::Result {
                snapshot = snapshot.with_observations(self.value_observations(
                    target.storage_path().expect("a result publication has its planned storage coordinate")));
            }

            // Publication and replay share the produced snapshot. Both apply physical retirements
            // at the end of this same operation, including those raised by installing its payload.
            let snapshot = std::sync::Arc::new(snapshot);
            let value = value.captured_value(snapshot.as_ref().clone());

            self.publish_value_graph(state, target, value.clone());

            self.execution.publish_captured_output(state.execution.as_mut().expect("publication retains its paired execution state"),
                invocation, &destination, value);

            completion.publish(destination, snapshot);
        }
    }

    fn value_observations<'a>(&'a self, coordinate: &'a bray_bound_tree::StoragePath)
        -> impl Iterator<Item = (Vec<bray_bound_tree::StorageProjection>, bray_bound_tree::StorageObservationKind)> + 'a
    {
        self.input.footprint.observations(coordinate).map(|(path, kind)|
            (path.projections[coordinate.projections.len()..].to_vec(), kind))
            .chain(self.input.footprint.references.iter().filter(|(path, _)| coordinate.contains(path))
                .map(|(path, ty)| (path.projections[coordinate.projections.len()..].to_vec(), bray_bound_tree::StorageObservationKind::Reference(*ty))))
    }

    fn publish_value_graph(&self, state: &mut StorageFlowState, target: bray_bound_tree::StorageTarget,
        value: crate::ExecutionCondition,
    ) {
        use crate::ExecutionConditionKind as K;
        use bray_bound_tree::StorageProjection;
        use bray_symbols::CapturedValueOperation as Op;

        let mut pending = vec![(target, value, false)];

        while let Some((target, value, mut covered)) = pending.pop() {
            if !covered && let Some(storage) = value.inline_captured_storage() {
                state.memory.publish_captured_value(&target, storage);
                covered = true;
            }

            if value.is_reference() { continue; }

            for projection in value.component_projections() {
                let child = value.clone().project(projection);

                if projection == StorageProjection::OwnedTarget {
                    let K::StorageValue(Op::OwningStorage(pointer), contents) = child.computed_value().kind() else { continue; };

                    let descriptor = value.inline_captured_storage().map(|storage| storage.project(*pointer));

                    let Some(descriptor) = descriptor else { continue; };

                    let Some(regions) = descriptor.address_regions() else { continue; };

                    for region in regions {
                        let elements = region.project(&[StorageProjection::AnyElement]);
                        let mut values = Vec::new();
                        let mut prefix = &contents[0];

                        loop {
                            match prefix.computed_value().kind() {
                                K::StorageValue(Op::Append, operands) => {
                                    values.push(operands[1].clone());
                                    prefix = &operands[0];
                                }
                                K::StorageValue(Op::EmptyPrefix, _) => {
                                    if descriptor.known_address() == Some(&region) { state.memory.clear_value(&elements); }
                                    break;
                                }
                                _ => {
                                    if let Some(bounds) = prefix.inline_captured_storage() {
                                        state.memory.publish_captured_value(&elements, bounds);
                                    } else { state.memory.lose_captured_information(&elements); }

                                    break;
                                }
                            }
                        }

                        pending.extend(values.into_iter().map(|value| (elements.clone(), value, false)));
                    }
                } else {
                    pending.push((target.project(&[projection]), child, covered));
                }
            }
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn record_capture_retirements(&mut self,
        execution: &mut crate::analysis::guarantee::ExecutionState,
        mutation: bray_bound_tree::StorageMutation, phase: bray_bound_tree::StorageValuePhase, boundary: bray_bound_tree::StorageValueBoundary,
        retirements: std::collections::BTreeSet<(bray_bound_tree::StorageTarget, bray_bound_tree::StorageValueRetirement)>,
    ) {
        if retirements.is_empty() { return; }

        let values = retirements.into_iter().collect::<Vec<_>>();

        execution.retire_captured_storage(&values);

        if self.publish { self.capture_retirements.entry((mutation, phase, boundary)).or_default().extend(values); }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn publish_capture_retirements(&mut self,
        state: &mut StorageFlowState, operation: crate::analysis::model::AnalysisOperationKind, boundary: bray_bound_tree::StorageValueBoundary,
    ) {
        let retirements = state.memory.take_retirements();
        let execution = state.execution.as_mut().expect("storage retirement retains its paired execution state");

        self.record_capture_retirements(execution, bray_bound_tree::StorageMutation::Operation(operation.occurrence()),
            operation.storage_value_phase(), boundary, retirements);
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn resolve_value_captures(&mut self,
        state: &StorageFlowState, operation: crate::analysis::model::AnalysisOperationKind,
    ) {
        if !matches!(operation, crate::analysis::model::AnalysisOperationKind::Call {
            phase: crate::analysis::model::AnalysisCallPhase::Input, ..
        }) && self.execution.single_phase_invocation(operation).is_none() { return; }

        let invocation = operation.occurrence();

        let BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) = invocation else { return; };

        let Some(selection) = self.execution.semantics.selections().expression(expression) else { return; };

        let mut captures = Vec::new();

        for input in selection.inputs() {
            let Some(ordinal) = selection.input_ordinal(input.root()) else { continue; };

            let Some(source) = self.invocation_reference(state, invocation, input.root()) else { continue; };

            let coordinate = self.invocation_place(invocation, input.root()).or_else(|| self.invocation_result(invocation))
                .expect("checked input capture retains planned storage coordinates");

            let value = state.memory.capture_evaluated_value(&source, bray_bound_tree::StorageTarget::storage(coordinate.clone()), input.ty(self.request.semantic_values()))
                .with_observations(self.value_observations(&coordinate));

            let referent = match self.request.semantic_values().type_data(input.ty(self.request.semantic_values())).as_ref() {
                TypeData::Borrow { target: ty, .. } => value.known_address().map(|target|
                    (*ty, std::sync::Arc::new(state.memory.capture_reached_value(target)
                        .with_observations(self.input.footprint.typed_observations(*ty).map(|(path, kind)| (path.to_vec(), kind)))))),
                _ => None,
            };

            let ty = input.ty(self.request.semantic_values());

            let element_bounds = self.input.raw_buffers.get(&self.request.semantic_values().unborrowed_type(ty)).and_then(|fields| {
                let pointer = bray_bound_tree::StorageProjection::ProductField(fields.pointer);

                let regions = if matches!(self.request.semantic_values().type_data(ty).as_ref(), TypeData::Borrow { .. }) {
                    value.address_regions()?.into_iter().map(|owner| state.memory.owned_regions(&owner, pointer))
                        .collect::<Option<Vec<_>>>()?.into_iter().flatten().collect()
                } else {
                    value.project(pointer).address_regions()?
                };

                let coordinate = bray_bound_tree::StorageTarget::storage(coordinate.clone());

                Some((pointer, std::sync::Arc::new(state.memory.capture_element_bounds(&regions, &coordinate))))
            });

            captures.push(bray_bound_tree::StorageValueCapture { input: ordinal, element_bounds,
                value: std::sync::Arc::new(value), referent });
        }

        let captures = captures.into();

        if self.publish {
            self.value_captures.entry(invocation).and_modify(|previous| {
                let mut completion = bray_bound_tree::StorageCompletion::default().with_captures(std::sync::Arc::clone(previous));

                completion.merge(&bray_bound_tree::StorageCompletion::default().with_captures(std::sync::Arc::clone(&captures)));
                *previous = completion.captures().into();
            }).or_insert(captures);
        } else {
            self.value_captures.insert(invocation, captures);
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn resolve_call_values(&mut self,
        execution: &crate::analysis::guarantee::ExecutionState, invocation: BoundExecutionSite,
    ) {
        let Some(contracts) = self.dependencies.calls().get(&invocation) else { return; };

        if contracts.deferred_effects().is_some() { return; }

        let values: std::sync::Arc<[StorageValueTransition]> =
            self.execution.invocation_value_transitions(execution, invocation, contracts.captured_values()).into();

        if self.publish {
            self.value_transitions.entry(invocation).and_modify(|previous| {
                if previous != &values { *previous = StorageValueTransition::join_reachable(previous, &values); }
            }).or_insert(values);
        } else {
            self.value_transitions.insert(invocation, values);
        }
    }
}
