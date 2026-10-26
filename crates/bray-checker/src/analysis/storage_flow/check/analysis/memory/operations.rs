use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::Target;
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::{
    AnyBoundNodeId, CheckedMemoryOperationKind, MemoryOperationDecision, MemoryOperationStatus,
    Refinement, RefinementKind, StorageAccessPurpose,
};

const fn conservative_memory_status(
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
    pub(in crate::analysis::storage_flow::check::analysis) fn apply_memory_operation<
        'refinements,
    >(
        &mut self,
        state: &mut StorageFlowState,
        node: AnyBoundNodeId,
        refinements: impl Iterator<Item = &'refinements Refinement> + Clone,
    ) -> Option<MemoryOperationStatus> {
        let AnyBoundNodeId::Expression(expression) = node else {
            return None;
        };

        let memory = self.memory;

        let operation = memory.operation(expression)?;

        let proven = self.memory_evidence;
        let mut write_target = None;

        let status = if !state.reachable {
            MemoryOperationStatus::Unreachable
        } else if state.recovered {
            MemoryOperationStatus::Recovered
        } else if crate::memory::execution::requires_trust(&operation.kind())
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

                        self.expression_target(state, argument).map(|place| {
                            if self.publish
                                && matches!(
                                    operation.kind(),
                                    CheckedMemoryOperationKind::Write { .. }
                                )
                            {
                                write_target = state.memory.value_target(&place, element);
                            }

                            (state.memory.reached_storage(&place), element, purpose)
                        })
                    });

            let status = self.apply_valid_memory_operation(state, operation, proven);

            if status == MemoryOperationStatus::Valid && proven != Some(true)
                && let Some((targets, element, purpose)) = targets {
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

        if self.publish
            && status == MemoryOperationStatus::Valid
            && matches!(operation.kind(), CheckedMemoryOperationKind::Write { .. })
        {
            self.record_state_target(
                bray_bound_tree::StorageStateTarget::MemoryWrite(expression),
                write_target,
            );
        }

        self.is_recovered |= status == MemoryOperationStatus::Recovered;

        if self.publish {
            self.memory_decisions
                .entry(expression)
                .and_modify(|current| *current = conservative_memory_status(*current, status))
                .or_insert(status);

            self.add_memory_diagnostic(state, operation, status);
        }

        Some(status)
    }

    fn apply_valid_memory_operation(
        &self,
        state: &mut StorageFlowState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        proven: Option<bool>,
    ) -> MemoryOperationStatus {
        let arguments = operation.arguments();

        let argument = |index| {
            arguments
                .get(index)
                .and_then(|argument| self.expression_target(state, *argument))
        };

        let result = || self.expression_result_storage(operation.expression());

        match operation.kind() {
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

                if let Some(source) = self.expression_reference(state, arguments[0]) {
                    source.transfer(&mut state.memory, result.clone());

                    state
                        .memory
                        .expose_address(&result, pointee, operation.expression());
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
                if let (Some(source), Some(result)) = (argument(0), result()) {
                    state.memory.offset(&source, result);
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
                    if let Some(source) = self.expression_reference(state, arguments[pointer]) {
                        source.transfer(&mut state.memory, result);
                    } else {
                        state.memory.clear(&result);
                    }
                }
            }
            CheckedMemoryOperationKind::Read { pointee, kind }
            | CheckedMemoryOperationKind::VolatileRead { pointee, kind, .. } => {
                let Some(pointer) = argument(0) else {
                    return MemoryOperationStatus::Recovered;
                };

                return state.memory.read(
                    &pointer,
                    pointee,
                    kind,
                    proven.map(|_| operation.expression()),
                    result(),
                );
            }
            CheckedMemoryOperationKind::Write { pointee }
            | CheckedMemoryOperationKind::VolatileWrite { pointee, .. } => {
                let Some(pointer) = argument(0) else {
                    return MemoryOperationStatus::Recovered;
                };

                let value = self.expression_reference(state, arguments[1]);

                return state
                    .memory
                    .write(&pointer, pointee, operation.expression(), value);
            }
            CheckedMemoryOperationKind::Copy { pointee, .. } => {
                let [source, destination, ..] = arguments else {
                    return MemoryOperationStatus::Recovered;
                };

                let Some(source) = self.expression_target(state, *source) else {
                    return MemoryOperationStatus::Recovered;
                };

                let Some(destination) = self.expression_target(state, *destination) else {
                    return MemoryOperationStatus::Recovered;
                };

                return state.memory.copy(
                    &source,
                    &destination,
                    pointee,
                    operation.expression(),
                    proven,
                );
            }
            CheckedMemoryOperationKind::RawAllocate
            | CheckedMemoryOperationKind::RawBufferAllocate
            | CheckedMemoryOperationKind::Allocate => {
                let Some(result) = result() else {
                    return MemoryOperationStatus::Recovered;
                };

                let allocation = state.memory.fresh(result.clone(), operation.expression());

                if operation.kind() == CheckedMemoryOperationKind::RawBufferAllocate {
                    self.retain_allocated_buffer_pointer(state, &result, allocation);
                }
            }
            CheckedMemoryOperationKind::RawDeallocate | CheckedMemoryOperationKind::Deallocate => {
                let Some(pointer) = argument(0) else {
                    return MemoryOperationStatus::Recovered;
                };

                return state.memory.deallocate(&pointer, operation.expression());
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

        match operation.kind() {
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

                let value = self.expression_reference(state, arguments[0]);

                return state
                    .memory
                    .write_value(payload, element, operation.expression(), value);
            }
            CheckedMemoryOperationKind::UninitPointer { element, .. } => {
                let Some(payload) = self.uninit_target(state, arguments[0]) else {
                    return MemoryOperationStatus::Recovered;
                };

                if let Some(result) = result() {
                    state.memory.point_at(
                        payload,
                        result,
                        element,
                        bray_symbols::DependencyReferenceKind::Address,
                    );
                }
            }
            CheckedMemoryOperationKind::UninitWrite { element } => {
                let Some(payload) = self.uninit_target(state, arguments[0]) else {
                    return MemoryOperationStatus::Recovered;
                };

                let value = self.expression_reference(state, arguments[1]);

                let status = state.memory.write_value(
                    payload.clone(),
                    element,
                    operation.expression(),
                    value,
                );

                if status == MemoryOperationStatus::Valid
                    && let Some(result) = result()
                {
                    state.memory.point_at(
                        payload,
                        result,
                        element,
                        bray_symbols::DependencyReferenceKind::Borrow,
                    );
                }

                return status;
            }
            CheckedMemoryOperationKind::UninitAssumeInitialized { element }
            | CheckedMemoryOperationKind::UninitMove { element } => {
                let Some(payload) = self.uninit_target(state, arguments[0]) else {
                    return MemoryOperationStatus::Recovered;
                };

                return state.memory.read_value(
                    payload,
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
