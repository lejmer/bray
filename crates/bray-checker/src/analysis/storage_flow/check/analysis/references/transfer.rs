use bray_bound_tree::{SelectedPropagation, SelectedPropagationBoundary, StorageProjection};
use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Place, Source, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::BoundExecutionSite;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    fn generator_storage(&self, expression: bray_bound_tree::BoundExpressionId) -> Option<(Place, bool)> {
        if !self.request.view().expression(expression)?.accumulates_generator_result() {
            return None;
        }

        let slot = self.storage.value_slot(expression)?;
        let ty = self.storage.identity_type(slot)?;

        match self.request.semantic_values().type_data(ty).as_ref() {
            bray_symbols::TypeData::Generator(_) => Some((Place::root(slot), true)),
            bray_symbols::TypeData::Array { .. } => Some((Place::root(slot), false)),
            _ => None,
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn acquire_generator_storage(
        &self,
        state: &mut StorageFlowState,
        expression: bray_bound_tree::BoundExpressionId,
    ) {
        let Some((place, allocated)) = self.generator_storage(expression) else { return; };

        state.result_dependencies.abandon_transfer(self.storage, place.root);

        if allocated {
            state.initialize_path(&place);
            state.memory.acquire_owned_backing(place, expression);
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn append_generator_element(
        &self,
        state: &mut StorageFlowState,
        expression: bray_bound_tree::BoundExpressionId,
        generator: bray_bound_tree::BoundExpressionId,
    ) {
        let Some((place, allocated)) = self.generator_storage(generator) else { return; };

        let Some(bray_bound_tree::BoundExpression::ControlTransfer(transfer)) = self.request.view().expression(expression) else { return; };

        let Some(operand) = transfer.operand() else { return; };

        state.live.insert(place.root);

        if let Some(source) = self.expression_reference(state, operand) {
            let owner = Target::storage(place);

            let prefix = if allocated {
                state.memory.owned_backing(&owner).expect("a generator yield retains its acquired backing")
            } else { &owner };

            let element = prefix.project(&[StorageProjection::AnyElement]);

            state.memory.accumulate_value(&element, &source);
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn transfer_scoped_storage(
        &self,
        state: &mut StorageFlowState,
        transfer: crate::analysis::model::ScopedResultTransfer,
    ) {
        use crate::analysis::model::ScopedResultDestination;

        let source = transfer.source.map(Place::root);
        let payload = source.as_ref().map(|source| Target::storage(source.clone()).project(&[transfer.projection]));

        match transfer.destination {
            ScopedResultDestination::Capability(identity) => {
                let destination = Place::root(identity);

                state.memory.begin(identity);
                state.initialize_path(&destination);
                state.memory.transfer_value(payload.as_ref().expect("capability is a selected entry result"), destination);
            }
            ScopedResultDestination::Region { slot, displaced } => {
                let destination = Place::root(slot);

                if let Some(displaced) = displaced {
                    let previous = Place::root(displaced);

                    state.memory.begin(displaced);
                    state.initialize_path(&previous);
                    state.memory.transfer_value(&Target::storage(destination.clone()), previous);
                }

                state.initialize_path(&destination);
                state.memory.clear(&destination);

                if let Some(payload) = &payload {
                    state.memory.transfer_value(payload, Place { root: slot, projections: [transfer.projection].into() });
                }
            }
            ScopedResultDestination::Incident { .. } | ScopedResultDestination::Discard => {}
        }

        if let Some(source) = source {
            state.memory.clear(&source);
            state.move_complete_storage(source.root);
        }

        self.execution.transfer_scoped_result(state.execution.as_mut().expect("reached scoped result retains execution facts"), transfer);
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn transfer_memory_result_state(
        &self,
        state: &mut StorageFlowState,
        node: bray_bound_tree::AnyBoundNodeId,
    ) {
        if let bray_bound_tree::AnyBoundNodeId::Pattern(pattern) = node {
            for (binding, expression, access) in
                crate::storage::pattern_binding_sources(self.request.view(), self.storage, pattern)
            {
                let Some(bray_bound_tree::StorageBinding::Identity(destination)) =
                    self.storage.binding(binding)
                else {
                    continue;
                };

                let path = crate::storage::expression_storage_projections(self.storage, expression, access);

                let source = if self.expression_is_borrowed(expression) && path.is_some_and(<[_]>::is_empty) {
                    // The initializer formed a descriptor. Its reached object is
                    // not a stored descriptor that can be copied into the local.
                    self.expression_reference(state, expression)
                } else if let Some(result) = self.expression_result_storage(expression) {
                    path.map(|path| Source::Value(Target::storage(result).project(path)))
                } else {
                    self.input.reference_target(self.request.semantic_values(), &state.memory, self.storage, access)
                        .map(Source::Value)
                };

                let source_type = self.storage.access(access).expect("pattern source is planned").reached_type();
                let target_type = self.storage.identity_type(destination).expect("pattern binding has its checked type");
                let destination = Place::root(destination);

                if let Some(wrapping) = crate::storage::nullable_wrapping(self.request.semantic_values(), source_type, target_type)
                    && let Some(source) = source {
                    source.transfer(&mut state.memory, Place { root: destination.root, projections: wrapping.into() });
                }

                let value = state.execution.as_ref().map(|execution| self.execution.storage_value(execution, expression, access));

                self.observe_transferred_value(state, value.as_ref(), &destination, target_type);
            }

            return;
        }

        let bray_bound_tree::AnyBoundNodeId::Expression(expression) = node else {
            return;
        };

        if let Some(result) = self.expression_result_storage(expression)
            && matches!(self.storage.identity(result.root), Some(bray_bound_tree::StorageIdentity::Evaluated(_)))
        {
            let source = self.storage.node_plans(expression.into()).find_map(|plan| {
                matches!(self.storage.effective_purpose(plan),
                    bray_bound_tree::StorageAccessPurpose::Copy
                        | bray_bound_tree::StorageAccessPurpose::Move
                        | bray_bound_tree::StorageAccessPurpose::ValueTransfer)
                    .then(|| self.input.reference_target(
                        self.request.semantic_values(), &state.memory, self.storage, plan.access()))
                    .flatten()
            });

            if let Some(source) = source {
                // The move's existing evaluated payload survives until finish_value_transfers.
                state.memory.transfer_value(&source, result);
            }
        }

        if let Some(bray_bound_tree::SemanticSelection::Propagation(selection)) =
            self.execution.semantics.selections().expression(expression)
            && let Some(bray_bound_tree::BoundExpression::Structured(operation)) =
                self.request.view().expression(expression)
            && let Some(result) = self.expression_result_storage(expression)
            && let Some(Source::Value(source)) = self
                .expression_reference(
                    state,
                    *operation
                        .operands()
                        .first()
                        .expect("checked propagation retains its operand"),
                )
        {
            // Normal continuation transfers the selected payload into a new slot.
            // Reuse the checked projection, including nullable and RunResult cases.
            state
                .memory
                .transfer_value(&source.project(&[selection.success_projection()]), result);
        }

        if let Some(fields) = self.input.reference_constructions.get(&expression) {
            for (source, source_type, destination, ty) in fields {
                // Each transition owns its destination while sharing the prepared projection path.
                self.transfer_reference_producer(state, *source, *source_type, destination.clone(), *ty);
            }
        }

        if self.memory.operation(expression).is_none()
            && let Some(result) = self.expression_result_storage(expression)
            && let Some(source) = self.execution.reference_value_source(expression)
            && let Some(source) = self.expression_reference(state, source)
        {
            source.transfer(&mut state.memory, result);
        }

        if let Some(bray_bound_tree::BoundExpression::ControlTransfer(transfer)) =
            self.request.view().expression(expression)
        {
            if let Some(operand) = transfer.operand()
                && let Some(access) = self.storage.control_transfer_destination(expression)
                && let Some(destination) = self.storage.logical_path(access)
            {
                let ty = self.storage.access(access).expect("control transfer has its destination access").reached_type();

                self.transfer_reference_value(state, operand, destination.clone(), ty);
            }
        } else if matches!(self.request.root(), crate::CheckerUnitRoot::Expression(root) if root == expression)
        {
            let Some(bray_bound_tree::StorageBinding::Identity(result)) = self
                .storage
                .binding(bray_bound_tree::StorageBindingTarget::Result)
            else {
                panic!("planned expression body has result storage");
            };

            let ty = self
                .execution
                .semantics
                .types()
                .expression(expression)
                .expect("expression body has a checked result type")
                .ty();

            let result = Place::root(result);

            state.initialize_path(&result);
            self.transfer_reference_value(state, expression, result, ty);
        }
    }

    pub(in crate::analysis::storage_flow) fn propagate_reference_result(
        &self,
        state: &mut StorageFlowState,
        expression: bray_bound_tree::BoundExpressionId,
    ) {

        let Some(bray_bound_tree::SemanticSelection::Propagation(selection)) =
            self.execution.semantics.selections().expression(expression)
        else {
            unreachable!("propagation edge retains its checked selection");
        };

        let (destination, region) = match selection.boundary() {
            Some(SelectedPropagationBoundary::Callable) => {
                let Some(bray_bound_tree::StorageBinding::Identity(result)) = self
                    .storage
                    .binding(bray_bound_tree::StorageBindingTarget::Result)
                else {
                    panic!("propagation retains its callable result storage");
                };

                (Place::root(result), None)
            }
            Some(SelectedPropagationBoundary::YieldRegion(_)) => {
                let target = self
                    .execution
                    .yield_targets
                    .get(&expression)
                    .expect("propagation retains its selected yield target");

                (self.expression_result_storage(*target)
                    .expect("yield propagation retains its result storage"), Some(*target))
            }
            None => return,
        };

        crate::dependency::ResultTransfer::new(
            self.request, self.execution.semantics.types(), self.execution.semantics.selections(),
            self.patterns, self.values, self.storage, &mut state.result_dependencies,
        ).propagate_failure(expression, region);

        state.initialize_path(&destination);
        state.memory.clear(&destination);

        let absent = Target::storage(destination.clone())
            .project(&[selection.success_projection()])
            .storage_path()
            .expect("inline result payload retains its storage path")
            .clone();

        self.mark_absent_references(state, &absent);

        let SelectedPropagation::Result {
            error: (variant, field),
            error_conversion,
            ..
        } = selection
        else {
            return;
        };

        let Some(bray_bound_tree::BoundExpression::Structured(operation)) =
            self.request.view().expression(expression)
        else {
            unreachable!("checked propagation retains its structured expression");
        };

        let operand = *operation
            .operands()
            .first()
            .expect("propagation retains its operand");

        let projection = StorageProjection::ActiveUnionPayloadField {
            variant: *variant,
            field: *field,
        };

        let mut projections = vec![projection];

        match error_conversion.target() {
            bray_bound_tree::ConversionTarget::Identity
            | bray_bound_tree::ConversionTarget::CallableContract => {}
            bray_bound_tree::ConversionTarget::NullablePresent => {
                projections.push(StorageProjection::NullableValue)
            }
            // A value-changing conversion does not establish the source's reference identity.
            _ => return,
        }

        if let Some(Source::Value(source)) =
            self.expression_reference(state, operand)
        {
            let destination = Place {
                root: destination.root,
                projections: destination
                    .projections
                    .iter()
                    .copied()
                    .chain(projections)
                    .collect(),
            };

            let source = source.project(&[projection]);

            state.memory.transfer_value(&source, destination);
        }
    }

    pub(super) fn reference_wrapping(
        &self,
        expression: bray_bound_tree::BoundExpressionId,
        target: bray_symbols::TypeId,
    ) -> Option<Vec<StorageProjection>> {
        let source_type = self
            .execution
            .semantics
            .types()
            .expression(expression)
            .expect("transferred value has a checked type")
            .ty();

        crate::storage::nullable_wrapping(self.request.semantic_values(), source_type, target)
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn observe_transferred_value(
        &self,
        state: &mut StorageFlowState,
        value: Option<&crate::ExecutionCondition>,
        destination: &Place,
        target: bray_symbols::TypeId,
    ) -> bool {
        if let Some(value) = value {

            for payload in self
                .input
                .variant_payloads
                .get(destination)
                .into_iter()
                .flatten()
            {
                if let Some(StorageProjection::ActiveUnionPayloadField {
                    variant,
                    ..
                }) = payload.projections.last()
                    && value.clone().matches_variant(*variant)
                        == crate::ExecutionCondition::boolean(false)
                {
                    self.mark_absent_references(state, payload);
                }
            }
        }

        if matches!(self.request.semantic_values().type_data(target).as_ref(), bray_symbols::TypeData::Nullable(_))
            && matches!(value.map(crate::ExecutionCondition::kind), Some(crate::ExecutionConditionKind::Literal(value))
                if matches!(value.kind(), bray_symbols::ConstantValueKind::NullableAbsent))
        {
            self.mark_absent_references(
                state,
                &Place {
                    root: destination.root,
                    projections: destination
                        .projections
                        .iter()
                        .copied()
                        .chain([StorageProjection::NullableValue])
                        .collect(),
                },
            );

            return true;
        }

        false
    }

    fn mark_absent_references(&self, state: &mut StorageFlowState, absent: &Place) {
        std::sync::Arc::make_mut(&mut state.definitely_absent).insert(
            absent.clone(),
            bray_bound_tree::StorageAbsence::NoPayload,
        );

        // The captured absence moves with the containing inactive payload.
        for path in self.input.reference_slots(absent) {
            state.memory.no_payload(path);
        }
    }

    fn transfer_reference_value(
        &self,
        state: &mut StorageFlowState,
        expression: bray_bound_tree::BoundExpressionId,
        destination: Place,
        target: bray_symbols::TypeId,
    ) {
        let source_type = self.execution.semantics.types().expression(expression)
            .expect("transferred value has a checked type").ty();

        self.transfer_reference_producer(state, expression.into(), source_type, destination, target);
    }

    fn transfer_reference_producer(
        &self,
        state: &mut StorageFlowState,
        producer: BoundExecutionSite,
        source_type: bray_symbols::TypeId,
        destination: Place,
        target: bray_symbols::TypeId,
    ) {
        let source = match producer {
            BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) => self.expression_reference(state, expression),
            BoundExecutionSite::CallInput { .. } => self.invocation_result(producer)
                .map(|place| Source::Value(Target::storage(place))),
            _ => unreachable!("reference construction retains an evaluated value producer"),
        };

        let value = state.execution.as_ref().map(|execution| self.execution.invocation_value(execution, producer));

        // The transfer owns its converted destination path.
        if let Some(projections) = crate::storage::nullable_wrapping(self.request.semantic_values(), source_type, target)
            && let Some(source) = source
        {
            source.transfer(
                &mut state.memory,
                Place {
                    root: destination.root,
                    projections: destination
                        .projections
                        .iter()
                        .copied()
                        .chain(projections)
                        .collect(),
                },
            );
        } else {
            state.memory.clear(&destination);
        }

        self.observe_transferred_value(state, value.as_ref(), &destination, target);
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn prepare_intrinsic_result_references(
        &self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
    ) {
        let Some(destination) = self.invocation_result(occurrence) else { return; };

        self.stage_result_reference_bounds(state, occurrence, &destination, &[], |memory, execution| {
            let operation = occurrence.expression().and_then(|expression| self.memory.operation(expression));

            match operation {
                Some(operation) if matches!(operation.kind(), bray_bound_tree::CheckedMemoryOperationKind::BorrowFrom { .. }) => {
                    // The loan's first input retains its lifetime. Only the pointer
                    // input describes the storage it can reach.
                    execution.and_then(|execution| self.captured_memory_operand(execution, operation, operation.arguments()[1]))
                        .and_then(bray_bound_tree::StorageCapturedValue::address_regions)
                        .unwrap_or_default()
                }
                _ => self.invocation_roots(
                    memory,
                    execution.and_then(|execution| execution.admitted_call(occurrence)),
                )
                .regions()
                .to_vec(),
            }
        });
    }

    pub(super) fn stage_result_reference_bounds<I: IntoIterator<Item = Target>>(
        &self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
        destination: &Place,
        inactive: &[Place],
        opaque_inputs: impl FnOnce(
            &crate::analysis::storage_flow::memory::State,
            Option<&crate::analysis::guarantee::ExecutionState>,
        ) -> I,
    ) {
        let root = destination.projections.is_empty().then(|| self.storage.storage_type(destination.root))
            .flatten().and_then(|ty| crate::representation::reference_target_type(self.request, ty))
            .map(|ty| (destination.clone(), ty));

        let contained = self.input.value_references.get(&destination.root);

        if root.is_none() && contained.is_none_or(|paths| paths.is_empty()) {
            return;
        }

        let execution = state.execution.as_ref();

        let references = contained.into_iter().flat_map(|paths| paths.iter())
            .filter(|(path, _)| destination.contains_storage(path)
                && !inactive.iter().any(|payload| payload.contains_storage(path))
                && (path.projections.is_empty() || !state.no_payload_paths(path.root).any(|absent| absent.contains_storage(path))))
            .map(|(path, ty)| (path.clone(), *ty))
            .chain(root.filter(|(path, _)| !contained.is_some_and(|paths| paths.contains_key(path))))
            .collect::<Vec<_>>();

        state.memory.stage_returned_references(occurrence, references, |memory| opaque_inputs(memory, execution));
    }
}
