use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::Target;
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::{
    AnyBoundNodeId, CheckedMemoryOperationKind, MemoryOperationDecision, MemoryOperationStatus,
    Refinement, RefinementKind, StorageAccessPurpose, StorageOutcomeRows,
};

pub(in crate::analysis::storage_flow::check::analysis) const fn conservative_memory_status(
    current: MemoryOperationStatus,
    candidate: MemoryOperationStatus,
) -> MemoryOperationStatus {
    if memory_status_rank(candidate) > memory_status_rank(current) {
        candidate
    } else {
        current
    }
}

const fn memory_status_rank(status: MemoryOperationStatus) -> u8 {
    match status {
        MemoryOperationStatus::Unreachable => 0,
        MemoryOperationStatus::Valid => 1,
        MemoryOperationStatus::Recovered => 2,
        MemoryOperationStatus::MissingTrustedEvidence => 3,
        MemoryOperationStatus::InvalidatedAllocation => 4,
        MemoryOperationStatus::UninitializedRawStorage => 5,
        MemoryOperationStatus::OutstandingObligations => 6,
    }
}

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn observe_memory_read(
        &mut self,
        state: &StorageFlowState,
        execution: &mut crate::analysis::guarantee::ExecutionState,
        point: &crate::analysis::model::AnalysisOperation,
        rows: Option<&StorageOutcomeRows>,
        invalidation: Option<&mut bray_bound_tree::StorageInvalidation>,
    ) {
        let Some(expression) = point.kind().explicit_attempt_expression() else {
            return;
        };

        let Some(operation) = self.memory.operation(expression) else {
            return;
        };

        let Some(pointee) = operation.kind().moving_read_type() else {
            return;
        };

        let target = self.captured_memory_read(execution, expression)
            .and_then(|read| state.memory.captured_value_target(read.binding()?, pointee));

        if self.execution.observe_memory_read(execution, expression, target.as_ref())
            && !matches!(self.request.semantic_values().type_data(pointee).as_ref(), bray_symbols::TypeData::Borrow { .. })
            && let Some(invalidation) = invalidation {
            let snapshots = match invalidation {
                bray_bound_tree::StorageInvalidation::All { normal_snapshots, .. }
                | bray_bound_tree::StorageInvalidation::Observed { normal_snapshots, .. } => normal_snapshots,
                bray_bound_tree::StorageInvalidation::Unchanged => unreachable!("read capture follows flow resolution"),
            };

            for (path, kind) in self.input.footprint.typed_observations(pointee) {
                if kind.is_immutable_inline() || matches!(kind,
                    bray_bound_tree::StorageObservationKind::Discriminant | bray_bound_tree::StorageObservationKind::Initialization) {
                    snapshots.insert((bray_bound_tree::StorageSnapshotRoot::PendingResult, path.into()), kind);
                }
            }
        }

        if self.publish {
            self.record_state_target((point.id(), rows), bray_bound_tree::StorageStateTarget::MemoryRead(expression), target);
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn captured_memory_read(
        &self,
        execution: &crate::analysis::guarantee::ExecutionState,
        expression: bray_bound_tree::BoundExpressionId,
    ) -> Option<bray_bound_tree::StorageCapturedValue> {
        let operation = self.memory.operation(expression)?;

        self.captured_memory_operand(execution, operation, operation.arguments()[0]).cloned()
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn apply_memory_operation<
        'refinements,
    >(
        &mut self,
        state: &mut StorageFlowState,
        point: &crate::analysis::model::AnalysisOperation,
        rows: Option<&StorageOutcomeRows>,
        refinements: impl Iterator<Item = &'refinements Refinement> + Clone,
    ) -> Option<(MemoryOperationStatus, Option<Target>)> {
        let AnyBoundNodeId::Expression(expression) = point.kind().node() else {
            return None;
        };

        let memory = self.memory;

        let operation = memory.operation(expression)?;

        let proven = state.execution.as_ref().and_then(|execution| execution.admitted_call(expression.into()))
            .and_then(|entry| entry.memory_access)
            .map(|proof| proof == crate::execution_guarantees::MemoryAccessProof::EmptyCopy);

        let pointer = match operation.kind() {
            CheckedMemoryOperationKind::Read { .. } | CheckedMemoryOperationKind::VolatileRead { .. } => {
                self.captured_memory_read(state.execution.as_ref()
                    .expect("completed read retains its admitted execution"), expression)
            }
            CheckedMemoryOperationKind::Write { .. } | CheckedMemoryOperationKind::VolatileWrite { .. } => {
                let execution = state.execution.as_ref().expect("completed write retains its admitted execution");

                self.captured_memory_operand(execution, operation, operation.arguments()[0]).cloned()
            }
            CheckedMemoryOperationKind::RawDeallocate | CheckedMemoryOperationKind::Deallocate => {
                self.captured_deallocation_pointer(state.execution.as_ref()
                    .expect("completed deallocation retains its admitted execution"), operation)
            }
            _ => None,
        };

        let pointer = pointer.as_ref().and_then(bray_bound_tree::StorageCapturedValue::binding);

        let mut write_target = None;

        let status = if !state.reachable {
            MemoryOperationStatus::Unreachable
        } else if state.recovered {
            MemoryOperationStatus::Recovered
        } else if crate::memory::execution::requires_trust(operation.kind())
            && proven.is_none()
            && !refinements
                .clone()
                .any(|refinement| matches!(refinement.kind(), RefinementKind::TrustBoundary(_)))
        {
            MemoryOperationStatus::MissingTrustedEvidence
        } else {
            let targets =
                self.memory_mutation(operation)
                    .and_then(|(argument, element, purpose)| {
                        if matches!(
                            operation.kind(),
                            CheckedMemoryOperationKind::UninitWrite { .. }
                                | CheckedMemoryOperationKind::UninitMove { .. }
                                | CheckedMemoryOperationKind::UninitAssumeInitialized { .. }
                        ) {
                            // These operations change protected contents, not the validity
                            // of their Uninit owner. The memory state owns that transition.
                            return None;
                        }

                        let binding = pointer.or_else(|| {
                            let execution = state.execution.as_ref().expect("memory mutation retains admitted inputs");

                            self.captured_memory_operand(execution, operation, argument)?.binding()
                        })?;

                        if matches!(operation.kind(), CheckedMemoryOperationKind::Write { .. }) {
                            write_target = state.memory.captured_value_target(binding, element);
                        }

                        Some((state.memory.reached_captured_storage(binding), element, purpose))
                    });

            let status = self.apply_valid_memory_operation(state, operation, proven, pointer);

            if status == MemoryOperationStatus::Valid
                && proven != Some(true)
                && let Some((targets, element, purpose)) = targets
            {
                for (place, storage_type, definite) in targets {
                    let same_type = storage_type == Some(element);

                    if purpose == StorageAccessPurpose::Move || !same_type {
                        state.record_move(
                            place,
                            operation.expression(),
                            storage_type.unwrap_or(element),
                            definite && same_type,
                        );
                    } else if definite {
                        state.initialize_path(&place);
                    }
                }
            }

            status
        };

        if status != MemoryOperationStatus::Valid {
            write_target = None;
        }

        if self.publish && matches!(operation.kind(), CheckedMemoryOperationKind::Write { .. }) {
            // Completion and the durable operation record need the same reached target.
            self.record_state_target(
                (point.id(), rows),
                bray_bound_tree::StorageStateTarget::MemoryWrite(expression),
                write_target.clone(),
            );
        }

        self.is_recovered |= status == MemoryOperationStatus::Recovered;

        if self.publish {
            self.memory_decisions
                .entry(expression)
                .and_modify(|current| *current = conservative_memory_status(*current, status))
                .or_insert(status);

            self.add_memory_diagnostic(state, operation, status, pointer);
        }

        Some((status, write_target))
    }

    fn apply_valid_memory_operation(
        &self,
        state: &mut StorageFlowState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        proven: Option<bool>,
        pointer: Option<&bray_bound_tree::StorageValueBinding>,
    ) -> MemoryOperationStatus {
        let arguments = operation.arguments();

        let result = || self.expression_result_storage(operation.expression());
        let execution = state.execution.as_ref().expect("memory operation retains its evaluated inputs");

        let operand = |index| self.captured_memory_operand(execution, operation, arguments[index])
            .cloned().map(crate::analysis::storage_flow::memory::Source::Captured);

        match *operation.kind() {
            CheckedMemoryOperationKind::UninitNew { .. }
            | CheckedMemoryOperationKind::UninitFrom { .. }
            | CheckedMemoryOperationKind::UninitPointer { .. }
            | CheckedMemoryOperationKind::UninitWrite { .. }
            | CheckedMemoryOperationKind::UninitAssumeInitialized { .. }
            | CheckedMemoryOperationKind::UninitMove { .. } => {
                return self.apply_uninit_operation(state, operation);
            }
            CheckedMemoryOperationKind::Address { pointee, .. } => {
                let Some(result) = result() else {
                    return MemoryOperationStatus::Recovered;
                };

                if let Some(value) = self.captured_memory_operand(execution, operation, arguments[0])
                    .and_then(bray_bound_tree::StorageCapturedValue::binding)
                {
                    state.memory.publish_exposed_address(&result, pointee, operation.expression(), value);
                } else {
                    state.memory.clear(&result);
                }
            }
            CheckedMemoryOperationKind::Null { .. } => {
                if let Some(result) = result() {
                    state.memory.null(result);
                }
            }
            CheckedMemoryOperationKind::IsNull { .. }
            | CheckedMemoryOperationKind::LayoutQuery { .. } => {}
            CheckedMemoryOperationKind::Offset { .. } => {
                if let (Some(source), Some(result)) = (operand(0), result()) {
                    source.transfer(&mut state.memory, result.clone());
                    state.memory.offset(&Target::storage(result.clone()), result);
                }
            }
            CheckedMemoryOperationKind::BorrowFrom { .. }
            | CheckedMemoryOperationKind::ByteAddress { .. }
            | CheckedMemoryOperationKind::Reinterpret { .. }
            | CheckedMemoryOperationKind::CallableFromPointer { .. }
            | CheckedMemoryOperationKind::PointerFromCallable { .. } => {
                let pointer = usize::from(matches!(
                    operation.kind(),
                    CheckedMemoryOperationKind::BorrowFrom { .. }
                ));

                if let Some(result) = result() {
                    // Unknown physical provenance remains unknown after a checked conversion.
                    // It is not syntax recovery and does not erase the body's other facts.
                    if let Some(source) = operand(pointer) {
                        if let CheckedMemoryOperationKind::BorrowFrom { pointee, .. } =
                            operation.kind()
                        {
                            source.transfer(&mut state.memory, result.clone());
                            state.memory.borrow_view(&result, *pointee);
                        } else {
                            source.transfer(&mut state.memory, result);
                        }
                    } else {
                        state.memory.clear(&result);
                    }
                }
            }
            CheckedMemoryOperationKind::Read { pointee, kind }
            | CheckedMemoryOperationKind::VolatileRead { pointee, kind, .. } => {
                return state.memory.read_captured(
                    pointer,
                    pointee,
                    kind,
                    proven.map(|_| operation.expression()),
                    result(),
                );
            }
            CheckedMemoryOperationKind::Write { pointee }
            | CheckedMemoryOperationKind::VolatileWrite { pointee, .. } => {
                return state
                    .memory
                    .write_captured(pointer, pointee, operation.expression(), operand(1));
            }
            CheckedMemoryOperationKind::Copy { pointee, .. } => {
                let [source, destination, ..] = arguments else {
                    return MemoryOperationStatus::Recovered;
                };

                let source = self.captured_memory_operand(execution, operation, *source)
                    .and_then(bray_bound_tree::StorageCapturedValue::binding);

                let destination = self.captured_memory_operand(execution, operation, *destination)
                    .and_then(bray_bound_tree::StorageCapturedValue::binding);

                return state.memory.copy_captured(
                    source,
                    destination,
                    pointee,
                    operation.expression(),
                    proven,
                );
            }
            CheckedMemoryOperationKind::RawBufferAllocate => {
                let Some(result) = result() else {
                    return MemoryOperationStatus::Recovered;
                };

                let allocation = state.memory.allocate_current(&result, operation.expression());

                self.retain_allocated_buffer_pointer(state, &result, allocation);
            }
            CheckedMemoryOperationKind::RawAllocate | CheckedMemoryOperationKind::Allocate => {
                let Some(result) = result() else {
                    return MemoryOperationStatus::Recovered;
                };

                let pointer = if matches!(operation.kind(), CheckedMemoryOperationKind::Allocate) {
                    let (field, _) = self.input.allocation_pointer.expect("allocation field is prepared");

                    let mut pointer = result.clone();

                    pointer.projections = result.projections.iter().copied()
                        .chain([bray_bound_tree::StorageProjection::ProductField(field)]).collect();

                    pointer
                } else {
                    result.clone()
                };

                state.memory.fresh(pointer, operation.expression());
            }
            CheckedMemoryOperationKind::RawDeallocate | CheckedMemoryOperationKind::Deallocate => {
                return state.memory.deallocate(pointer, operation.expression());
            }
            CheckedMemoryOperationKind::RawBufferPointer
            | CheckedMemoryOperationKind::RawBufferSparePointer { .. }
            | CheckedMemoryOperationKind::RawBufferInitializedSlice
            | CheckedMemoryOperationKind::RawBufferInitializedSliceMut => {
                return self.apply_buffer_view(state, operation, result());
            }
            CheckedMemoryOperationKind::ByteBufferFill
            | CheckedMemoryOperationKind::ByteBufferRead
            | CheckedMemoryOperationKind::SequenceLength
            | CheckedMemoryOperationKind::RawBufferCapacity
            | CheckedMemoryOperationKind::RawBufferInitializedCount
            | CheckedMemoryOperationKind::RawBufferPush { .. }
            | CheckedMemoryOperationKind::RawBufferPop { .. }
            | CheckedMemoryOperationKind::RawBufferSetInitializedCount
            | CheckedMemoryOperationKind::RawBufferRelease { .. }
            | CheckedMemoryOperationKind::CallbackState { .. }
            | CheckedMemoryOperationKind::AtomicInitialize { .. }
            | CheckedMemoryOperationKind::AtomicLoad { .. }
            | CheckedMemoryOperationKind::AtomicStore { .. }
            | CheckedMemoryOperationKind::AtomicExchange { .. }
            | CheckedMemoryOperationKind::AtomicCompareExchange { .. }
            | CheckedMemoryOperationKind::AtomicFetch { .. }
            | CheckedMemoryOperationKind::AtomicWait { .. }
            | CheckedMemoryOperationKind::AtomicNotify { .. } => {}
            CheckedMemoryOperationKind::ExposeAddress { .. }
            | CheckedMemoryOperationKind::FromExposedAddress { .. }
            | CheckedMemoryOperationKind::CompareAddress { .. }
            | CheckedMemoryOperationKind::Fence { .. }
            | CheckedMemoryOperationKind::CatastrophicAbort
            | CheckedMemoryOperationKind::DebuggerTrap
            | CheckedMemoryOperationKind::UnreachableTermination
            | CheckedMemoryOperationKind::SpinLoopHint
            | CheckedMemoryOperationKind::TargetFeatureEnabled { .. }
            | CheckedMemoryOperationKind::InlineAssembly { .. } => {}
            CheckedMemoryOperationKind::RawBufferReplace { .. }
            | CheckedMemoryOperationKind::RawBufferRelocate { .. } => {}
        }

        MemoryOperationStatus::Valid
    }

    fn apply_uninit_operation(
        &self,
        state: &mut StorageFlowState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
    ) -> MemoryOperationStatus {
        let arguments = operation.arguments();
        let result = || self.expression_result_storage(operation.expression());
        let execution = state.execution.as_ref().expect("protected storage operation retains its evaluated inputs");

        let operand = |index| self.captured_memory_operand(execution, operation, arguments[index])
            .cloned().map(crate::analysis::storage_flow::memory::Source::Captured);

        match *operation.kind() {
            CheckedMemoryOperationKind::UninitNew { element } => {
                if let Some(result) = result() {
                    state.memory.uninit(
                        Target::storage(result)
                            .project(&[bray_bound_tree::StorageProjection::UninitPayload]),
                        element,
                    );
                }
            }
            CheckedMemoryOperationKind::UninitFrom { element } => {
                let Some(result) = result() else {
                    return MemoryOperationStatus::Recovered;
                };

                let payload = Target::storage(result)
                    .project(&[bray_bound_tree::StorageProjection::UninitPayload]);

                state.memory.uninit(payload.clone(), element);

                return state
                    .memory
                    .write_value(payload, element, operation.expression(), operand(0));
            }
            CheckedMemoryOperationKind::UninitPointer { element, .. } => {
                let Some(payload) = self.memory_uninit_address(state, execution, operation, arguments[0]) else {
                    return MemoryOperationStatus::Recovered;
                };

                if let Some(result) = result() {
                    state.memory.point_at_addresses(
                        payload,
                        result,
                        element,
                        bray_symbols::DependencyReferenceKind::Address,
                    );
                }
            }
            CheckedMemoryOperationKind::UninitWrite { element } => {
                let Some(payload) = self.memory_uninit_address(state, execution, operation, arguments[0]) else {
                    return MemoryOperationStatus::Recovered;
                };

                let payload = bray_bound_tree::StorageValueBinding::from(payload);
                let status = state.memory.write_captured(
                    Some(&payload),
                    element,
                    operation.expression(),
                    operand(1),
                );

                if status == MemoryOperationStatus::Valid
                    && let Some(result) = result()
                {
                    state.memory.point_at_addresses(
                        payload.address.expect("the projected borrow retains its address descriptor"),
                        result,
                        element,
                        bray_symbols::DependencyReferenceKind::Borrow,
                    );
                }

                return status;
            }
            CheckedMemoryOperationKind::UninitAssumeInitialized { element }
            | CheckedMemoryOperationKind::UninitMove { element } => {
                let Some(payload) = self.memory_uninit_address(state, execution, operation, arguments[0]) else {
                    return MemoryOperationStatus::Recovered;
                };

                return state.memory.read_captured(
                    Some(&payload.into()),
                    element,
                    bray_bound_tree::MemoryReadKind::Move,
                    Some(operation.expression()),
                    result(),
                );
            }
            _ => unreachable!("protected storage operation was selected by the caller"),
        }

        MemoryOperationStatus::Valid
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn memory_decisions(
        &self,
    ) -> impl Iterator<Item = MemoryOperationDecision> + '_ {
        self.memory_decisions
            .iter()
            .map(|(expression, status)| MemoryOperationDecision::new(*expression, *status))
    }
}
