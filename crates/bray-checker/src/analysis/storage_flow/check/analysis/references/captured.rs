use std::sync::Arc;

use bray_bound_tree::{AnalysisOperationId, BoundExecutionSite, StorageOutcomeRows, StorageValueBoundary, StorageValueCapture, StorageValueDestination, StorageValueTransition};
use bray_symbols::TypeData;

use super::super::core::StorageFlowCollector;
use crate::analysis::storage_flow::memory::Source;
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::CheckerRequestContext;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn record_current_write(&self, state: &mut StorageFlowState, target: &bray_bound_tree::StorageTarget) {
        if target.is_exact()
            && matches!(target.origin, bray_bound_tree::StorageOrigin::Entry { layer: 1, kind: bray_symbols::DependencyReferenceKind::Borrow, .. })
            && let Some(path) = target.inline_state_path()
            && self.formal_root(path.root).is_some()
            && !state.written_inputs.iter().any(|written| written.contains(&path))
        {
            state.written_inputs.retain(|written| !path.contains(written));
            state.written_inputs.insert(path);
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn preserve_captured_observations(&self,
        execution: &crate::analysis::guarantee::ExecutionState, invocation: BoundExecutionSite,
        invalidation: &mut bray_bound_tree::StorageInvalidation,
    ) {
        let snapshots = match invalidation {
            bray_bound_tree::StorageInvalidation::All { normal_snapshots, .. }
            | bray_bound_tree::StorageInvalidation::Observed { normal_snapshots, .. } => normal_snapshots,
            bray_bound_tree::StorageInvalidation::Unchanged => unreachable!("captured outputs retain resolved observations"),
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

            let produced = match destination {
                StorageValueDestination::Result => state.memory.capture_produced_reference(&target),
                StorageValueDestination::Current { installed: true, .. } => Some(state.memory.capture_reached_value(&target)),
                _ => None,
            };

            let mut snapshot = produced.unwrap_or_else(|| {
                // Install the actual component captures before reading the new
                // destination. The preceding contents do not describe this value.
                self.publish_value_graph(state, target.clone(), value.clone(), destination == StorageValueDestination::Result);

                state.memory.capture_reached_value(&target)
            });

            if destination == StorageValueDestination::Result {
                snapshot = snapshot.with_observations(self.value_observations(
                    target.storage_path().expect("a result publication has its planned storage coordinate")));
            }

            // Publication and replay share the produced snapshot. Both apply physical retirements
            // at the end of this same operation, including those raised by installing its payload.
            let snapshot = std::sync::Arc::new(snapshot);
            let value = value.captured_value(snapshot.clone());

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
        -> impl Iterator<Item = (&'a [bray_bound_tree::StorageProjection], bray_bound_tree::StorageObservationKind)> + 'a
    {
        self.input.footprint.observations(coordinate)
            .chain(self.input.footprint.references_below(coordinate)
                .map(|(path, ty)| (path, bray_bound_tree::StorageObservationKind::Reference(ty))))
            .map(move |(path, kind)| (&path.projections[coordinate.projections.len()..], kind))
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn publish_value_graph(&self, state: &mut StorageFlowState, target: bray_bound_tree::StorageTarget,
        value: crate::ExecutionCondition, result: bool,
    ) {
        use crate::ExecutionConditionKind as K;
        use bray_bound_tree::StorageProjection;
        use bray_symbols::CapturedValueOperation as Op;

        let mut pending = vec![(target, value, false, result)];

        while let Some((target, value, mut covered, result)) = pending.pop() {
            if !covered && let Some(storage) = value.inline_captured_storage() {
                if result && target.is_exact() {
                    state.memory.complete_result_capture(&target, storage);
                } else {
                    state.memory.publish_captured_value(&target, storage);
                }

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
                            if let Some(captured) = prefix.inline_captured_storage() {
                                if prefix.prefix_starts_empty() && descriptor.known_address() == Some(&region) {
                                    state.memory.clear_value(&elements);
                                }

                                state.memory.include_captured_bounds(&elements, captured);
                                break;
                            }

                            // A portable outcome can introduce new elements. Bind their actual
                            // operands only as far back as the first reached prefix snapshot.
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
                                    state.memory.lose_captured_information(&elements);
                                    break;
                                }
                            }
                        }

                        pending.extend(values.into_iter().map(|value| (elements.clone(), value, false, false)));
                    }
                } else {
                    pending.push((target.project(&[projection]), child, covered, result));
                }
            }
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn record_capture_retirements(&mut self,
        execution: &mut crate::analysis::guarantee::ExecutionState,
        mutation: bray_bound_tree::StorageMutation, boundary: StorageValueBoundary,
        retirements: Vec<(bray_bound_tree::StorageTarget, bray_bound_tree::StorageValueRetirement)>,
    ) {
        if retirements.is_empty() { return; }

        execution.retire_captured_storage(&retirements);

        self.publish_retirements(mutation, boundary, retirements);
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn publish_retirements(&mut self,
        mutation: bray_bound_tree::StorageMutation, boundary: StorageValueBoundary,
        values: impl IntoIterator<Item = (bray_bound_tree::StorageTarget, bray_bound_tree::StorageValueRetirement)>,
    ) {
        let mut values = values.into_iter().peekable();

        if self.publish && values.peek().is_some() {
            self.capture_retirements.entry((mutation, boundary)).or_default().extend(values);
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn publish_capture_retirements(&mut self,
        state: &mut StorageFlowState, (operation, rows): (AnalysisOperationId, Option<&StorageOutcomeRows>), boundary: StorageValueBoundary,
    ) {
        let retirements = state.memory.take_retirements();

        if retirements.is_empty() { return; }

        let execution = state.execution.as_mut().expect("storage retirement retains its paired execution state");

        execution.retire_captured_storage(&retirements);
        self.publish_operation_retirements((operation, rows), boundary, retirements);
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn publish_operation_retirements(
        &mut self,
        (operation, rows): (AnalysisOperationId, Option<&StorageOutcomeRows>),
        boundary: StorageValueBoundary,
        retirements: Vec<(bray_bound_tree::StorageTarget, bray_bound_tree::StorageValueRetirement)>,
    ) {
        if self.publish && !retirements.is_empty() {
            self.operation_retirements.entry(((operation, boundary), rows.cloned().unwrap_or_default()))
                .or_default().extend(retirements);
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn resolve_value_captures(&mut self,
        state: &mut StorageFlowState, operation: &crate::analysis::model::AnalysisOperation,
    ) -> Option<Arc<[StorageValueCapture]>> {
        let kind = operation.kind();

        if !matches!(kind, crate::analysis::model::AnalysisOperationKind::Call {
            phase: crate::analysis::model::AnalysisCallPhase::Input, ..
        }) && self.execution.single_phase_invocation(kind).is_none() { return None; }

        let invocation = kind.occurrence();

        let expression = invocation.expression()?;
        let selection = self.execution.semantics.selections().expression(expression)?;
        let values = self.request.semantic_values();

        let carries_frame = self.dependencies.calls().get(&invocation).is_some_and(|call| {
            call.captured_values().iter().flat_map(|row| row.bindings.iter())
                .any(|(_, value)| value.callees().next().is_some())
        });

        let sole_input = match invocation {
            BoundExecutionSite::CallInput { ordinal, .. } => {
                let selected = selection.input(ordinal).expect("materialization retains its selected input");

                selected.expression().map(|source| {
                    let ty = match selected {
                        // Receiver staging evaluates the selected loan even
                        // when its source expression names an owned value.
                        bray_bound_tree::SelectedInput::Receiver(_) => selected.ty(values),
                        _ => self.execution.semantics.types().expression(source)
                            .expect("evaluated conversion source retains its checked type").ty(),
                    };

                    (bray_symbols::DependencySubjectRoot::Receiver, bray_symbols::SymbolOrdinal::new(0), ty)
                })
            }
            _ => self.storage.implicit_input_plan(invocation).map(|plan| {
                let ordinal = bray_symbols::SymbolOrdinal::new(0);

                let root = selection.invocation_input_root(invocation, ordinal)
                    .expect("implicit storage input has a receiver-first ordinal");

                (root, ordinal, crate::storage::implicit_input_type(values, self.storage, plan))
            }),
        };

        let operation_inputs = match (invocation, selection) {
            (BoundExecutionSite::Node(_), bray_bound_tree::SemanticSelection::Operation(operation))
                if !matches!(operation, bray_bound_tree::SelectedOperation::Construction(_)) => {
                crate::dependency::operation_inputs(self.request.view(), expression, operation)
            }
            _ => Vec::new(),
        };

        let ordinary_inputs = selection.inputs().filter(|_| sole_input.is_none()).filter_map(|input| {
            let root = input.root();
            let ordinal = selection.invocation_input_ordinal(invocation, root)?;

            Some((root, ordinal, input.ty(values)))
        });

        let operation_inputs = operation_inputs.into_iter().filter_map(|(root, input)| {
            let ordinal = selection.input_ordinal(root)?;

            let ty = if let Some(plan) = input.borrow_plan(self.storage) {
                crate::storage::implicit_input_type(values, self.storage, plan)
            } else {
                self.storage.access(input.access(self.storage)?)?.reached_type()
            };

            Some((root, ordinal, ty))
        });

        let inputs = ordinary_inputs.chain(sole_input).chain(operation_inputs);

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

            let value = if (sole_input.is_some() || matches!(source, Source::Borrow(_))) && borrowed.is_some() {
                // The descriptor is the passed loan. Its contents are captured separately.
                value.with_observations([(&[][..], bray_bound_tree::StorageObservationKind::Reference(ty))])
            } else {
                value.with_observations(self.value_observations(&coordinate))
            };

            let value = if carries_frame
                && let Some(site) = selection.invocation_input_site(invocation, root)
            {
                let support = bray_bound_tree::ReportDependencies {
                    subjects: self.liveness.retained_by(site).collect(),
                    requirements: state.result_dependencies.value_requirements(site).cloned().collect(),
                    ..Default::default()
                };

                value.with_dependency_support(Arc::new(support))
            } else {
                value
            };

            let referent = borrowed.and_then(|ty| capture_input_referent(
                &state.memory, &value, ty, self.input.footprint.typed_observations(ty),
            ));

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
            self.value_captures.entry((operation.id(), None)).and_modify(|known| {
                bray_bound_tree::merge_storage_value_captures(known, &captures);
            }).or_insert_with(|| Arc::clone(&captures));
        }

        Some(captures)
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn begin_value_transition(
        &mut self, memory: &crate::analysis::storage_flow::memory::State, execution: &mut crate::analysis::guarantee::ExecutionState,
        operation: &crate::analysis::model::AnalysisOperation, effects: &[crate::analysis::storage_flow::memory::Effect],
    ) {
        let invocation = operation.kind().occurrence();

        self.execution.prepare_failure_rows(execution, invocation);

        let values: Arc<[StorageValueTransition]> =
            if let BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) = invocation
            && let Some(operation) = self.memory.operation(expression)
        {
            self.resolve_memory_values(execution, operation, effects)
        } else if let Some(contract) = self.dependencies.calls().get(&invocation)
            && contract.deferred_effects().is_none()
        {
            let resolve = memory.reached_write_resolver(self.request.semantic_values());

            self.execution.invocation_value_transitions(execution, invocation, contract.captured_values(), &resolve).into()
        } else {
            return;
        };

        self.execution.prepare_captured_values(execution, invocation, &values);

        if self.publish {
            self.value_transitions.entry((operation.id(), None)).and_modify(|previous| {
                bray_bound_tree::merge_storage_value_transitions(previous, &values);
            }).or_insert(values);
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn prepare_frame_execution(
        &mut self,
        memory: &crate::analysis::storage_flow::memory::State,
        execution: &mut crate::analysis::guarantee::ExecutionState,
        operation: &crate::analysis::model::AnalysisOperation,
    ) {
        let publish = self.publish;
        let footprint = &self.input.footprint;
        let values = self.request.semantic_values();
        let captures = &mut self.value_captures;
        let transitions = &mut self.value_transitions;
        let resolve = memory.reached_write_resolver(values);
        let capture = |entry: &crate::ExecutionCallEvidence| entry.positional_arguments().filter_map(|(input, value)| {
            let descriptor = value.inline_captured_storage()?;
            let ty = descriptor.observations().find_map(|(path, kind)| match kind {
                bray_bound_tree::StorageObservationKind::Reference(ty) if path.is_empty() => Some(ty),
                _ => None,
            })?;
            let ty = crate::representation::borrow_target_type(values, ty)?;
            let referent = capture_input_referent(memory, descriptor, ty, footprint.typed_observations(ty))?;

            Some(StorageValueCapture {
                input, value: Arc::new(descriptor.clone()), referent: Some(referent), element_bounds: None,
            })
        }).collect();

        self.execution.prepare_frame_execution(execution, operation, Some((&resolve, &capture)), |producer, inputs, values| {
            if publish {
                let values = values.into();

                let key = (operation.id(), Some(producer.clone()));

                captures.entry(key.clone()).and_modify(|previous| {
                    bray_bound_tree::merge_storage_value_captures(previous, &inputs);
                }).or_insert_with(|| inputs.into());
                transitions.entry(key).and_modify(|previous| {
                    bray_bound_tree::merge_storage_value_transitions(previous, &values);
                }).or_insert(values);
            }
        });
    }

    pub(in crate::analysis::storage_flow) fn prepare_operation_outcomes(
        &mut self, state: &mut StorageFlowState, operation: &crate::analysis::model::AnalysisOperation,
    ) -> Vec<usize> {
        use crate::analysis::model::{AnalysisCallPhase, AnalysisOperationKind};

        let kind = operation.kind();

        if kind.awaited_expression().is_some() {
            let execution = state.execution.as_mut().expect("reachable await retains execution facts");

            self.prepare_frame_execution(&state.memory, execution, operation);

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

        self.begin_value_transition(&state.memory, execution, operation, &[]);

        self.execution.feasible_call_outputs(execution, invocation)
    }
}

fn capture_input_referent<'path>(
    memory: &crate::analysis::storage_flow::memory::State,
    descriptor: &bray_bound_tree::StorageCapturedValue,
    ty: bray_symbols::TypeId,
    observations: impl IntoIterator<Item = (&'path [bray_bound_tree::StorageProjection], bray_bound_tree::StorageObservationKind)>,
) -> Option<(bray_symbols::TypeId, Arc<bray_bound_tree::StorageCapturedValue>)> {
    let target = descriptor.known_address()?;
    let value = memory.capture_reached_value(target).with_observations(observations);

    Some((ty, Arc::new(value)))
}
