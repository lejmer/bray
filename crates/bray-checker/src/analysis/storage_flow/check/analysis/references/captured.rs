use std::sync::Arc;

use bray_bound_tree::{BoundExecutionSite, StorageValueCapture, StorageValueDestination, StorageValueTransition};
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

        let regions = outputs.possible_region_values()
            .map(|(target, value)| (target.clone(), value.clone()))
            .collect::<std::collections::BTreeSet<_>>();

        let outputs = outputs.destinations().filter_map(|destination|
            self.execution.captured_output(execution, invocation, destination).map(|value| (destination.clone(), value))).collect::<Vec<_>>();

        for (destination, value) in outputs {
            let target = match &destination {
                StorageValueDestination::Region { .. } => unreachable!("a possible region has no current value"),
                StorageValueDestination::Result => {
                    let Some(path) = self.invocation_result(invocation) else { continue; };

                    bray_bound_tree::StorageTarget::storage(path)
                }
                StorageValueDestination::Current { target, .. } => {
                    self.record_current_write(state, target);

                    target.clone()
                }
            };

            let produced = (destination == StorageValueDestination::Result)
                .then(|| state.memory.capture_produced_reference(&target)).flatten();

            let mut snapshot = produced.unwrap_or_else(|| {
                // Install the actual component captures before reading the new
                // destination. The preceding contents do not describe this value.
                self.publish_value_graph(state, target.clone(), value.clone());

                state.memory.capture_reached_value(&target)
            });

            if destination == StorageValueDestination::Result {
                snapshot = snapshot.with_observations(self.value_observations(
                    target.storage_path().expect("a result publication has its planned storage coordinate")));
            }

            // Publication and replay share the produced snapshot. Both apply physical retirements
            // at the end of this same operation, including those raised by installing its payload.
            let snapshot = std::sync::Arc::new(snapshot);
            let value = value.captured_value(snapshot.as_ref().clone());

            self.execution.publish_captured_output(state.execution.as_mut().expect("publication retains its paired execution state"),
                invocation, &destination, value);

            completion.publish(destination, snapshot);
        }

        for (region, value) in regions {
            if let Some(bounds) = value.inline_captured_storage() {
                state.memory.include_captured_bounds(&region, bounds);
            } else {
                state.memory.lose_captured_information(&region);
            }
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
        state: &mut StorageFlowState, operation: crate::analysis::model::AnalysisOperationKind,
    ) -> Option<Arc<[StorageValueCapture]>> {
        if !matches!(operation, crate::analysis::model::AnalysisOperationKind::Call {
            phase: crate::analysis::model::AnalysisCallPhase::Input, ..
        }) && self.execution.single_phase_invocation(operation).is_none() { return None; }

        let invocation = operation.occurrence();

        let expression = invocation.expression()?;
        let selection = self.execution.semantics.selections().expression(expression)?;
        let implicit = invocation != expression.into();
        let values = self.request.semantic_values();

        let implicit_input = if implicit {
            let plan = self.storage.implicit_input_plan(invocation)?;
            let ordinal = bray_symbols::SymbolOrdinal::new(0);

            let root = selection.invocation_input_root(invocation, ordinal)
                .expect("implicit storage input has a receiver-first ordinal");

            Some((root, ordinal, crate::storage::implicit_input_type(values, self.storage, plan)))
        } else {
            None
        };

        self.observe_call_reference_inputs(state, invocation);

        let inputs = selection.inputs().map(|input| {
            let root = input.root();

            let ordinal = selection.input_ordinal(root)
                .expect("selected input has a receiver-first ordinal");

            (root, ordinal, input.ty(values))
        }).chain(implicit_input);

        let mut captures = Vec::new();

        for (root, ordinal, ty) in inputs {
            let Some(source) = self.invocation_reference(state, invocation, root) else { continue; };

            let coordinate = self.invocation_place(invocation, root).or_else(|| self.invocation_result(invocation))
                .expect("checked input capture retains planned storage coordinates");

            let value = state.memory.capture_evaluated_value(&source, bray_bound_tree::StorageTarget::storage(coordinate.clone()), ty);

            let borrowed = match *values.type_data(ty) {
                TypeData::Borrow { target, .. } => Some(target),
                _ => None,
            };

            let value = if implicit && borrowed.is_some() {
                // The descriptor is the passed loan. Its contents are captured separately.
                value.with_observations([(Vec::new(), bray_bound_tree::StorageObservationKind::Reference(ty))])
            } else {
                value.with_observations(self.value_observations(&coordinate))
            };

            let referent = borrowed.and_then(|ty| value.known_address().map(|target|
                (ty, std::sync::Arc::new(state.memory.capture_reached_value(target)
                    .with_observations(self.input.footprint.typed_observations(ty).map(|(path, kind)| (path.to_vec(), kind)))))));

            let element_bounds = self.input.raw_buffers.get(&values.unborrowed_type(ty)).and_then(|fields| {
                let pointer = bray_bound_tree::StorageProjection::ProductField(fields.pointer);

                let regions = if borrowed.is_some() {
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

        let captures: Arc<[StorageValueCapture]> = captures.into();

        if self.publish {
            self.value_captures.entry(invocation).and_modify(|previous| {
                let mut completion = bray_bound_tree::StorageCompletion::default().with_captures(std::sync::Arc::clone(previous));

                completion.merge(&bray_bound_tree::StorageCompletion::default().with_captures(std::sync::Arc::clone(&captures)));
                *previous = completion.captures().into();
            }).or_insert_with(|| Arc::clone(&captures));
        }

        Some(captures)
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn begin_value_transition(
        &mut self, execution: &mut crate::analysis::guarantee::ExecutionState,
        invocation: BoundExecutionSite, effects: &[crate::analysis::storage_flow::memory::Effect],
    ) {
        self.execution.prepare_failure_rows(execution, invocation);

        let (values, portable_order): (Arc<[StorageValueTransition]>, bool) =
            if let BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) = invocation
            && let Some(operation) = self.memory.operation(expression)
        {
            (self.resolve_memory_values(execution, operation, effects), false)
        } else if let Some(contract) = self.dependencies.calls().get(&invocation)
            && contract.deferred_effects().is_none()
        {
            (self.execution.invocation_value_transitions(execution, invocation, contract.captured_values()).into(), true)
        } else {
            return;
        };

        self.execution.prepare_captured_values(execution, invocation, &values, portable_order);

        if self.publish {
            self.value_transitions.entry(invocation).and_modify(|previous| {
                bray_bound_tree::merge_storage_value_transitions(previous, &values);
            }).or_insert(values);
        }
    }

    pub(in crate::analysis::storage_flow) fn prepare_operation_outcomes(
        &mut self, state: &mut StorageFlowState, kind: crate::analysis::model::AnalysisOperationKind,
    ) -> Vec<usize> {
        use crate::analysis::model::{AnalysisCallPhase, AnalysisOperationKind};

        if kind.awaited_expression().is_some() {
            let execution = state.execution.as_mut().expect("reachable await retains execution facts");

            self.execution.prepare_await_execution(execution, kind);

            return Vec::new();
        }

        let AnalysisOperationKind::Call { invocation, phase: AnalysisCallPhase::Attempt } = kind else {
            return Vec::new();
        };

        if kind.explicit_attempt_expression().is_some_and(|expression| self.memory.operation(expression).is_some())
            || self.dependencies.calls().get(&invocation).is_none_or(|contract| {
                contract.deferred_effects().is_some() || contract.captured_values().len() <= 1
            })
        {
            return Vec::new();
        }

        let execution = state.execution.as_mut().expect("reachable call attempt has an execution state");

        self.begin_value_transition(execution, invocation, &[]);

        self.execution.feasible_call_outputs(execution, invocation)
    }
}
