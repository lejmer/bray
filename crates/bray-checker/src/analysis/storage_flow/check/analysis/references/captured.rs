use std::sync::Arc;

use bray_bound_tree::{
    AnalysisOperationId, BoundExecutionSite, StorageOutcomeRows, StorageValueBoundary,
    StorageValueCapture, StorageValueDestination, StorageValueTransition,
};
use bray_symbols::TypeData;

use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::Source;
use crate::analysis::storage_flow::model::StorageFlowState;

#[derive(Clone, Copy)]
pub(in crate::analysis::storage_flow::check::analysis) enum ValuePublication {
    Replacement,
    StagedResult,
    ReceiverWrite,
}

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn record_current_write(
        &self,
        state: &mut StorageFlowState,
        target: &bray_bound_tree::StorageTarget,
        ty: bray_symbols::TypeId,
    ) {
        if !target.is_exact() {
            return;
        }

        let bray_bound_tree::StorageOrigin::Entry {
            reference,
            layer: 1,
            kind: bray_symbols::DependencyReferenceKind::Borrow,
            projections,
        } = &target.origin
        else {
            return;
        };

        let Some(root) = self.formal_root(reference.root) else {
            return;
        };

        let path = reference
            .projections
            .iter()
            .chain(projections.iter())
            .copied()
            .collect::<Vec<_>>();

        let Some(path) = crate::storage::dependency_projections(self.request, &path) else {
            return;
        };

        state.record_written_input(
            bray_symbols::DependencySubject::new(root, path),
            bray_bound_tree::StorageObservedTarget::borrowed(target.clone(), ty),
        );
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn preserve_captured_observations(
        &self,
        execution: &crate::analysis::guarantee::ExecutionState,
        invocation: BoundExecutionSite,
        invalidation: &mut bray_bound_tree::StorageInvalidation,
    ) {
        let snapshots = match invalidation {
            bray_bound_tree::StorageInvalidation::All {
                normal_snapshots, ..
            }
            | bray_bound_tree::StorageInvalidation::Observed {
                normal_snapshots, ..
            } => normal_snapshots,
            bray_bound_tree::StorageInvalidation::Unchanged => {
                unreachable!("captured outputs retain resolved observations")
            }
        };

        let Some(outputs) = execution
            .admitted_call(invocation)
            .map(|entry| &entry.outputs)
        else {
            return;
        };

        for destination in outputs.destinations() {
            let Some(value) = outputs.common_value(destination) else {
                continue;
            };

            for (path, kind) in value.captured_observations() {
                if kind.is_immutable_inline()
                    || kind == bray_bound_tree::StorageObservationKind::Discriminant
                {
                    snapshots.insert(
                        (
                            bray_bound_tree::StorageSnapshotRoot::Produced(destination.clone()),
                            path.into(),
                        ),
                        kind,
                    );
                }
            }
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn publish_captured_outputs(
        &self,
        state: &mut StorageFlowState,
        invocation: BoundExecutionSite,
        completion: &mut bray_bound_tree::StorageCompletion,
    ) {
        let Some(execution) = state.execution.as_ref() else {
            return;
        };

        // Completed values and admitted outputs share immutable handles. The plan owns destinations.
        let Some(outputs) = execution
            .admitted_call(invocation)
            .map(|entry| &entry.outputs)
        else {
            return;
        };

        let regions = outputs
            .possible_region_values()
            .map(|(target, value)| (target.clone(), value.clone()))
            .collect::<std::collections::BTreeSet<_>>();

        let outputs = outputs
            .destinations()
            .filter_map(|destination| {
                self.execution
                    .captured_output(execution, invocation, destination)
                    .map(|value| (destination.clone(), value))
            })
            .collect::<Vec<_>>();

        for (destination, value) in outputs {
            let target = match &destination {
                StorageValueDestination::Region { .. } => {
                    unreachable!("a possible region has no current value")
                }
                StorageValueDestination::Result => {
                    let Some(path) = self.invocation_result(invocation) else {
                        continue;
                    };

                    bray_bound_tree::StorageTarget::storage(path)
                }
                StorageValueDestination::Current { target, ty, .. } => {
                    self.record_current_write(state, target, *ty);

                    target.clone()
                }
            };

            let produced = match destination {
                StorageValueDestination::Result => state.memory.capture_produced_reference(&target),
                StorageValueDestination::Current {
                    installed: true, ..
                } => Some(state.memory.capture_reached_value(&target)),
                _ => None,
            };

            let mut snapshot = produced.unwrap_or_else(|| {
                // Install the actual component captures before reading the new
                // destination. The preceding contents do not describe this value.
                let publication = if destination == StorageValueDestination::Result {
                    ValuePublication::StagedResult
                } else {
                    ValuePublication::Replacement
                };

                self.publish_value_graph(state, target.clone(), value.clone(), publication);

                state.memory.capture_reached_value(&target)
            });

            if destination == StorageValueDestination::Result {
                snapshot = snapshot.with_observations(
                    self.value_observations(
                        target
                            .storage_path()
                            .expect("a result publication has its planned storage coordinate"),
                    ),
                );
            }

            // Publication and replay share the produced snapshot. Both apply physical retirements
            // at the end of this same operation, including those raised by installing its payload.
            let snapshot = std::sync::Arc::new(snapshot);
            let value = value.captured_value(snapshot.clone());

            self.execution.publish_captured_output(
                state
                    .execution
                    .as_mut()
                    .expect("publication retains its paired execution state"),
                invocation,
                &destination,
                value,
            );

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

    fn value_observations<'a>(
        &'a self,
        coordinate: &'a bray_bound_tree::StoragePath,
    ) -> impl Iterator<
        Item = (
            &'a [bray_bound_tree::StorageProjection],
            bray_bound_tree::StorageObservationKind,
        ),
    > + 'a {
        self.input
            .footprint
            .observations(coordinate)
            .chain(
                self.input
                    .footprint
                    .references_below(coordinate)
                    .map(|(path, ty)| {
                        (path, bray_bound_tree::StorageObservationKind::Reference(ty))
                    }),
            )
            .map(move |(path, kind)| (&path.projections[coordinate.projections.len()..], kind))
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn publish_value_graph(
        &self,
        state: &mut StorageFlowState,
        target: bray_bound_tree::StorageTarget,
        value: crate::ExecutionCondition,
        publication: ValuePublication,
    ) {
        use crate::ExecutionConditionKind as K;
        use bray_bound_tree::StorageProjection;
        use bray_symbols::CapturedValueOperation as Op;

        let mut pending = vec![(target, value, false, publication)];

        while let Some((target, value, mut covered, publication)) = pending.pop() {
            if !covered && let Some(storage) = value.inline_captured_storage() {
                match publication {
                    ValuePublication::StagedResult | ValuePublication::ReceiverWrite
                        if target.is_exact() =>
                    {
                        state.memory.complete_value_capture(
                            &target,
                            storage,
                            matches!(publication, ValuePublication::StagedResult),
                        );
                    }
                    _ => state.memory.publish_captured_value(&target, storage),
                }

                covered = true;
            }

            if value.is_reference() {
                continue;
            }

            for projection in value.component_projections() {
                let child = value.clone().project(projection);

                if projection == StorageProjection::OwnedTarget {
                    let K::StorageValue(Op::OwningStorage(pointer), contents) =
                        child.computed_value().kind()
                    else {
                        continue;
                    };

                    if !covered && !matches!(publication, ValuePublication::StagedResult) {
                        continue;
                    }

                    let owner = crate::analysis::storage_flow::memory::MutationTarget::location(
                        target.clone(),
                    );

                    let Some(regions) = state.memory.owned_mutation_targets(&owner, *pointer)
                    else {
                        continue;
                    };

                    let exact = matches!(regions.as_slice(), [region] if region.address.as_ref() == Some(region.footprint()));

                    for region in regions {
                        let elements = region.footprint().project(&[StorageProjection::AnyElement]);
                        let mut values = Vec::new();
                        let mut prefix = &contents[0];

                        loop {
                            if let Some(captured) = prefix.inline_captured_storage() {
                                if prefix.prefix_starts_empty() && exact {
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
                                    if exact {
                                        state.memory.clear_value(&elements);
                                    }
                                    break;
                                }
                                _ => {
                                    state.memory.lose_captured_information(&elements);
                                    break;
                                }
                            }
                        }

                        pending.extend(values.into_iter().map(|value| {
                            (
                                elements.clone(),
                                value,
                                false,
                                ValuePublication::Replacement,
                            )
                        }));
                    }
                } else {
                    pending.push((target.project(&[projection]), child, covered, publication));
                }
            }
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn record_capture_retirements(
        &mut self,
        execution: &mut crate::analysis::guarantee::ExecutionState,
        mutation: bray_bound_tree::StorageMutation,
        boundary: StorageValueBoundary,
        retirements: Vec<(
            bray_bound_tree::StorageTarget,
            bray_bound_tree::StorageValueRetirement,
        )>,
    ) {
        if retirements.is_empty() {
            return;
        }

        execution.retire_captured_storage(&retirements);

        self.publish_retirements(mutation, boundary, retirements);
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn publish_retirements(
        &mut self,
        mutation: bray_bound_tree::StorageMutation,
        boundary: StorageValueBoundary,
        values: impl IntoIterator<
            Item = (
                bray_bound_tree::StorageTarget,
                bray_bound_tree::StorageValueRetirement,
            ),
        >,
    ) {
        let mut values = values.into_iter().peekable();

        if self.publish && values.peek().is_some() {
            self.capture_retirements
                .entry((mutation, boundary))
                .or_default()
                .extend(values);
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn publish_capture_retirements(
        &mut self,
        state: &mut StorageFlowState,
        (operation, rows): (AnalysisOperationId, Option<&StorageOutcomeRows>),
        boundary: StorageValueBoundary,
    ) {
        let retirements = state.memory.take_retirements();

        if retirements.is_empty() {
            return;
        }

        let execution = state
            .execution
            .as_mut()
            .expect("storage retirement retains its paired execution state");

        execution.retire_captured_storage(&retirements);

        self.publish_operation_retirements((operation, rows), boundary, retirements);
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn publish_operation_retirements(
        &mut self,
        (operation, rows): (AnalysisOperationId, Option<&StorageOutcomeRows>),
        boundary: StorageValueBoundary,
        retirements: Vec<(
            bray_bound_tree::StorageTarget,
            bray_bound_tree::StorageValueRetirement,
        )>,
    ) {
        if self.publish && !retirements.is_empty() {
            self.operation_retirements
                .entry(((operation, boundary), rows.cloned().unwrap_or_default()))
                .or_default()
                .extend(retirements);
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn resolve_value_captures(
        &mut self,
        state: &mut StorageFlowState,
        operation: &crate::analysis::model::AnalysisOperation,
    ) -> Option<Arc<[StorageValueCapture]>> {
        let kind = operation.kind();

        if !matches!(
            kind,
            crate::analysis::model::AnalysisOperationKind::Call {
                phase: crate::analysis::model::AnalysisCallPhase::Input,
                ..
            }
        ) && self.execution.single_phase_invocation(kind).is_none()
        {
            return None;
        }

        let invocation = kind.occurrence();

        let expression = invocation.expression()?;
        let selection = self
            .execution
            .semantics
            .selections()
            .expression(expression)?;
        let values = self.request.semantic_values();

        let carries_frame = self
            .dependencies
            .calls()
            .get(&invocation)
            .is_some_and(|call| {
                call.captured_values()
                    .iter()
                    .flat_map(|row| row.bindings.iter())
                    .any(|(_, value)| value.callees().next().is_some())
            });

        let sole_input = match invocation {
            BoundExecutionSite::CallInput { ordinal, .. } => {
                let selected = selection
                    .input(ordinal)
                    .expect("materialization retains its selected input");

                selected.expression().map(|source| {
                    let ty = match selected {
                        // Receiver staging evaluates the selected loan even
                        // when its source expression names an owned value.
                        bray_bound_tree::SelectedInput::Receiver(_) => selected.ty(values),
                        _ => self
                            .execution
                            .semantics
                            .types()
                            .expression(source)
                            .expect("evaluated conversion source retains its checked type")
                            .ty(),
                    };

                    (
                        bray_symbols::DependencySubjectRoot::Receiver,
                        bray_symbols::SymbolOrdinal::new(0),
                        ty,
                    )
                })
            }
            _ => self.storage.implicit_input_plan(invocation).map(|plan| {
                let ordinal = bray_symbols::SymbolOrdinal::new(0);

                let root = selection
                    .invocation_input_root(invocation, ordinal)
                    .expect("implicit storage input has a receiver-first ordinal");

                (
                    root,
                    ordinal,
                    crate::storage::implicit_input_type(values, self.storage, plan),
                )
            }),
        };

        let operation_inputs = match (invocation, selection) {
            (
                BoundExecutionSite::Node(_),
                bray_bound_tree::SemanticSelection::Operation(operation),
            ) if !matches!(
                operation,
                bray_bound_tree::SelectedOperation::Construction(_)
            ) =>
            {
                crate::dependency::operation_inputs(self.request.view(), expression, operation)
            }
            _ => Vec::new(),
        };

        let ordinary_inputs = selection
            .inputs()
            .filter(|_| sole_input.is_none())
            .filter_map(|input| {
                let root = input.root();
                let ordinal = selection.invocation_input_ordinal(invocation, root)?;

                Some((root, ordinal, input.ty(values)))
            });

        let operation_inputs = operation_inputs.into_iter().filter_map(|(root, input)| {
            let ordinal = selection.input_ordinal(root)?;

            let ty = if let Some(plan) = input.borrow_plan(self.storage) {
                crate::storage::implicit_input_type(values, self.storage, plan)
            } else {
                self.storage
                    .access(input.access(self.storage)?)?
                    .reached_type()
            };

            Some((root, ordinal, ty))
        });

        let inputs = ordinary_inputs.chain(sole_input).chain(operation_inputs);

        let mut captures = Vec::new();

        for (root, ordinal, ty) in inputs {
            let Some(source) = self.invocation_reference(state, invocation, root) else {
                continue;
            };

            let coordinate = self
                .invocation_place(invocation, root)
                .or_else(|| self.invocation_result(invocation))
                .expect("checked input capture retains planned storage coordinates");

            let value = state.memory.capture_evaluated_value(
                &source,
                bray_bound_tree::StorageTarget::storage(coordinate.clone()),
                ty,
            );

            let borrowed = match *values.type_data(ty) {
                TypeData::Borrow { kind, target } => Some((kind, target)),
                _ => None,
            };

            let value = if (sole_input.is_some() || matches!(source, Source::Borrow(_)))
                && borrowed.is_some()
            {
                // The descriptor is the passed loan. Its contents are captured separately.
                value.with_observations([(
                    &[][..],
                    bray_bound_tree::StorageObservationKind::Reference(ty),
                )])
            } else {
                value.with_observations(self.value_observations(&coordinate))
            };

            let mut value = if let Some(site) = selection.invocation_input_site(invocation, root)
                && (carries_frame
                    || value.binding().is_none_or(|binding| {
                        !binding.whole_complete() && binding.address.is_none()
                    })) {
                self.capture_operand_support(state, site, ty, value)
            } else {
                value
            };

            let referent = match borrowed
                .map(|(_, ty)| {
                    capture_input_referent(
                        self.request,
                        &state.memory,
                        &mut value,
                        ty,
                        self.input.footprint.typed_observations(ty),
                    )
                })
                .transpose()
            {
                Ok(referent) => referent.flatten(),
                Err(error) => {
                    self.fail_query(error);

                    return None;
                }
            };

            let element_bounds = self
                .input
                .raw_buffers
                .get(&values.unborrowed_type(ty))
                .and_then(|fields| {
                    let pointer = bray_bound_tree::StorageProjection::ProductField(fields.pointer);

                    let regions = if borrowed.is_some() {
                        value
                            .address_regions()?
                            .into_iter()
                            .map(|owner| state.memory.owned_regions(&owner, pointer))
                            .collect::<Option<Vec<_>>>()?
                            .into_iter()
                            .flatten()
                            .collect()
                    } else {
                        value.project(pointer).address_regions()?
                    };

                    let coordinate = bray_bound_tree::StorageTarget::storage(coordinate.clone());

                    Some((
                        pointer,
                        std::sync::Arc::new(
                            state.memory.capture_element_bounds(&regions, &coordinate),
                        ),
                    ))
                });

            captures.push(bray_bound_tree::StorageValueCapture {
                input: ordinal,
                borrow_kind: borrowed.map(|(kind, _)| kind),
                element_bounds,
                value: std::sync::Arc::new(value),
                referent,
            });
        }

        Some(self.publish_input_captures(operation.id(), captures))
    }

    fn capture_operand_support(
        &self,
        state: &StorageFlowState,
        site: BoundExecutionSite,
        ty: bray_symbols::TypeId,
        value: bray_bound_tree::StorageCapturedValue,
    ) -> bray_bound_tree::StorageCapturedValue {
        let subjects = self
            .inferred_value_subjects(site, Some(ty))
            .collect::<std::collections::BTreeSet<_>>();
        let requirements = state
            .result_dependencies
            .value_requirements(site)
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();

        if subjects.is_empty() && requirements.is_empty() {
            return value;
        }

        value.with_dependency_support(Arc::new(bray_bound_tree::ReportDependencies {
            subjects,
            requirements,
            ..Default::default()
        }))
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn inferred_value_subjects(
        &self,
        site: BoundExecutionSite,
        ty: Option<bray_symbols::TypeId>,
    ) -> impl Iterator<Item = bray_bound_tree::BoundDependencySubject> + '_ {
        let carries_no_borrows = ty.is_some_and(|ty| {
            self.input
                .footprint
                .carries_no_borrows(self.request.semantic_values(), ty)
        });

        self.liveness.retained_by(site).filter(move |subject| {
            !carries_no_borrows
                || !matches!(
                    subject,
                    bray_bound_tree::BoundDependencySubject::BorrowCapability(_)
                )
        })
    }

    fn publish_input_captures(
        &mut self,
        operation: AnalysisOperationId,
        captures: Vec<StorageValueCapture>,
    ) -> Arc<[StorageValueCapture]> {
        let captures: Arc<[StorageValueCapture]> = captures.into();

        if self.publish {
            self.value_captures
                .entry((operation, None))
                .and_modify(|known| {
                    bray_bound_tree::merge_storage_value_captures(known, &captures);
                })
                .or_insert_with(|| Arc::clone(&captures));
        }

        captures
    }

    fn capture_await_input(
        &mut self,
        state: &StorageFlowState,
        operation: AnalysisOperationId,
        expression: bray_bound_tree::BoundExpressionId,
    ) -> Arc<[StorageValueCapture]> {
        let Some(bray_bound_tree::BoundExpression::Await(awaited)) =
            self.request.view().expression(expression)
        else {
            unreachable!("a direct await operation retains its evaluated operand");
        };

        let operand = awaited.operand();

        let Some(coordinate) = self.argument_storage(operand) else {
            return Arc::from([]);
        };

        let ty = self
            .execution
            .semantics
            .types()
            .expression(operand)
            .expect("checked await retains its operand type")
            .ty();
        let coordinate = bray_bound_tree::StorageTarget::storage(coordinate);
        let source = self
            .expression_reference(state, operand)
            .unwrap_or_else(|| Source::Value(coordinate.clone()));

        let value = state
            .memory
            .capture_evaluated_value(&source, coordinate, ty)
            .with_observations(self.input.footprint.typed_observations(ty));

        let value = self.capture_operand_support(state, operand.into(), ty, value);

        self.publish_input_captures(
            operation,
            vec![StorageValueCapture {
                input: bray_symbols::SymbolOrdinal::new(0),
                borrow_kind: None,
                value: Arc::new(value),
                referent: None,
                element_bounds: None,
            }],
        )
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn begin_value_transition(
        &mut self,
        memory: &crate::analysis::storage_flow::memory::State,
        execution: &mut crate::analysis::guarantee::ExecutionState,
        operation: &crate::analysis::model::AnalysisOperation,
        effects: &[crate::analysis::storage_flow::memory::Effect],
    ) {
        let invocation = operation.kind().occurrence();

        self.execution.prepare_failure_rows(execution, invocation);

        let values: Arc<[StorageValueTransition]> = if let BoundExecutionSite::Node(
            bray_bound_tree::AnyBoundNodeId::Expression(expression),
        ) = invocation
            && let Some(operation) = self.memory.operation(expression)
        {
            self.resolve_memory_values(execution, operation, effects)
        } else if let Some(contract) = self.dependencies.calls().get(&invocation)
            && contract.deferred_effects().is_none()
        {
            let resolve = memory.reached_write_resolver(self.request.semantic_values());

            self.execution
                .invocation_value_transitions(
                    execution,
                    invocation,
                    contract.captured_values(),
                    &resolve,
                )
                .into()
        } else {
            return;
        };

        self.execution
            .prepare_captured_values(execution, invocation, &values);

        if self.publish {
            self.value_transitions
                .entry((operation.id(), None))
                .and_modify(|previous| {
                    bray_bound_tree::merge_storage_value_transitions(previous, &values);
                })
                .or_insert(values);
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn prepare_frame_execution(
        &mut self,
        memory: &crate::analysis::storage_flow::memory::State,
        execution: &mut crate::analysis::guarantee::ExecutionState,
        operation: &crate::analysis::model::AnalysisOperation,
        await_input: &[StorageValueCapture],
    ) {
        let publish = self.publish;
        let request = self.request;
        let footprint = &self.input.footprint;
        let values = self.request.semantic_values();
        let captures = &mut self.value_captures;
        let transitions = &mut self.value_transitions;
        let query_error = std::cell::Cell::new(None);
        let resolve = memory.reached_write_resolver(values);

        let capture = |entry: &crate::ExecutionCallEvidence| {
            entry
                .positional_arguments()
                .filter_map(|(input, value)| {
                    let mut descriptor = value.inline_captured_storage()?.clone();

                    let ty = descriptor
                        .observations()
                        .find_map(|(path, kind)| match kind {
                            bray_bound_tree::StorageObservationKind::Reference(ty)
                                if path.is_empty() =>
                            {
                                Some(ty)
                            }
                            _ => None,
                        })?;

                    let bray_symbols::TypeData::Borrow { kind, target: ty } = *values.type_data(ty)
                    else {
                        return None;
                    };

                    let referent = match capture_input_referent(
                        request,
                        memory,
                        &mut descriptor,
                        ty,
                        footprint.typed_observations(ty),
                    ) {
                        Ok(referent) => referent?,
                        Err(error) => {
                            query_error.set(Some(error));

                            return None;
                        }
                    };

                    Some(StorageValueCapture {
                        input,
                        borrow_kind: Some(kind),
                        value: Arc::new(descriptor),
                        referent: Some(referent),
                        element_bounds: None,
                    })
                })
                .collect()
        };

        self.execution.prepare_frame_execution(
            execution,
            operation,
            Some((&resolve, &capture, await_input)),
            |producer, inputs, values| {
                if publish {
                    let values = values.into();

                    let key = (operation.id(), Some(producer.clone()));

                    captures
                        .entry(key.clone())
                        .and_modify(|previous| {
                            bray_bound_tree::merge_storage_value_captures(previous, &inputs);
                        })
                        .or_insert_with(|| inputs.into());

                    transitions
                        .entry(key)
                        .and_modify(|previous| {
                            bray_bound_tree::merge_storage_value_transitions(previous, &values);
                        })
                        .or_insert(values);
                }
            },
        );

        if let Some(error) = query_error.into_inner() {
            self.fail_query(error);
        }
    }

    pub(in crate::analysis::storage_flow) fn prepare_operation_outcomes(
        &mut self,
        state: &mut StorageFlowState,
        operation: &crate::analysis::model::AnalysisOperation,
    ) -> Vec<usize> {
        use crate::analysis::model::{AnalysisCallPhase, AnalysisOperationKind};

        let kind = operation.kind();

        if let Some(expression) = kind.awaited_expression() {
            let capture = self.capture_await_input(state, operation.id(), expression);
            let execution = state
                .execution
                .as_mut()
                .expect("reachable await retains execution facts");

            self.prepare_frame_execution(&state.memory, execution, operation, &capture);

            return Vec::new();
        }

        let AnalysisOperationKind::Call {
            invocation,
            phase: AnalysisCallPhase::Attempt,
        } = kind
        else {
            return Vec::new();
        };

        if kind
            .explicit_attempt_expression()
            .is_some_and(|expression| self.memory.operation(expression).is_some())
            || self
                .dependencies
                .calls()
                .get(&invocation)
                .is_none_or(|contract| {
                    contract.deferred_effects().is_some() || contract.captured_values().len() <= 1
                })
        {
            return Vec::new();
        }

        let execution = state
            .execution
            .as_mut()
            .expect("reachable call attempt has an execution state");

        self.begin_value_transition(&state.memory, execution, operation, &[]);

        self.execution.feasible_call_outputs(execution, invocation)
    }
}

fn capture_input_referent<'path, C: CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    memory: &crate::analysis::storage_flow::memory::State,
    descriptor: &mut bray_bound_tree::StorageCapturedValue,
    ty: bray_symbols::TypeId,
    observations: impl Iterator<
        Item = (
            &'path [bray_bound_tree::StorageProjection],
            bray_bound_tree::StorageObservationKind,
        ),
    >,
) -> crate::CheckerQueryResult<
    Option<(
        bray_symbols::TypeId,
        Arc<bray_bound_tree::StorageCapturedValue>,
    )>,
    C::UpstreamError,
> {
    let Some(target) = descriptor.known_address() else {
        return Ok(None);
    };

    let value = memory.capture_observed_referent(target, observations);
    let enclosing = memory.enclosing_typed_view(request, target, ty)?;

    descriptor.set_enclosing_view(ty, enclosing);

    Ok(Some((ty, Arc::new(value))))
}
