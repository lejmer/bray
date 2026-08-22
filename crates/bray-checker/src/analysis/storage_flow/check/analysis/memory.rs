use std::collections::BTreeMap;

use bray_bound_tree::{
    AnyBoundNodeId, BoundExpressionId, CheckedMemoryOperationKind, MemoryOperationDecision,
    MemoryOperationStatus, Refinement, RefinementKind, StorageAccessPurpose, StorageIdentity,
    StorageIdentityId,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, SeverityKind,
};

use super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::model::StorageFlowState;

const fn operation_requires_trust(kind: CheckedMemoryOperationKind) -> bool {
    matches!(
        kind,
        CheckedMemoryOperationKind::UninitAssumeInitialized { .. }
            | CheckedMemoryOperationKind::UninitMove { .. }
            | CheckedMemoryOperationKind::BorrowFrom { .. }
            | CheckedMemoryOperationKind::Reinterpret { .. }
            | CheckedMemoryOperationKind::Read { .. }
            | CheckedMemoryOperationKind::Write { .. }
            | CheckedMemoryOperationKind::Copy { .. }
            | CheckedMemoryOperationKind::RawAllocate
            | CheckedMemoryOperationKind::RawDeallocate
            | CheckedMemoryOperationKind::Allocate
            | CheckedMemoryOperationKind::Deallocate
            | CheckedMemoryOperationKind::RawBufferInitializedSlice
            | CheckedMemoryOperationKind::RawBufferInitializedSliceMut
            | CheckedMemoryOperationKind::RawBufferSparePointer { .. }
            | CheckedMemoryOperationKind::RawBufferSetInitializedCount
            | CheckedMemoryOperationKind::RawBufferRelease { .. }
            | CheckedMemoryOperationKind::RawBufferReplace { .. }
            | CheckedMemoryOperationKind::RawBufferRelocate { .. }
            | CheckedMemoryOperationKind::ByteBufferFill
            | CheckedMemoryOperationKind::ByteSliceCopy
            | CheckedMemoryOperationKind::ByteBufferRead
            | CheckedMemoryOperationKind::CallbackState { .. }
            | CheckedMemoryOperationKind::VolatileRead { .. }
            | CheckedMemoryOperationKind::VolatileWrite { .. }
    )
}

fn replace_raw_state(
    state: &mut StorageFlowState,
    source: StorageIdentityId,
    destination: StorageIdentityId,
) {
    replace_map_entry(&mut state.raw_initialized, source, destination);
    replace_map_entry(&mut state.active_allocations, source, destination);
    replace_map_entry(&mut state.allocation_origins, source, destination);
    replace_map_entry(&mut state.invalidated_allocations, source, destination);
}

fn replace_map_entry<Value: Clone>(
    entries: &mut BTreeMap<StorageIdentityId, Value>,
    source: StorageIdentityId,
    destination: StorageIdentityId,
) {
    match entries.get(&source).cloned() {
        Some(value) => {
            entries.insert(destination, value);
        }
        None => {
            entries.remove(&destination);
        }
    }
}

fn has_raw_state(state: &StorageFlowState, storage: StorageIdentityId) -> bool {
    state.raw_initialized.contains_key(&storage)
        || state.active_allocations.contains_key(&storage)
        || state.allocation_origins.contains_key(&storage)
        || state.invalidated_allocations.contains_key(&storage)
}

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

const fn memory_diagnostic_kind(status: MemoryOperationStatus) -> Option<DiagnosticKind> {
    match status {
        MemoryOperationStatus::MissingTrustedEvidence => {
            Some(DiagnosticKind::CheckingMissingTrustedMemoryGuarantees)
        }
        MemoryOperationStatus::InvalidatedAllocation => {
            Some(DiagnosticKind::CheckingMemoryOperationAfterDeallocation)
        }
        MemoryOperationStatus::UninitializedRawStorage => {
            Some(DiagnosticKind::CheckingUninitializedRawStorage)
        }
        MemoryOperationStatus::OutstandingObligations => {
            Some(DiagnosticKind::CheckingDeallocationWithOutstandingObligations)
        }
        MemoryOperationStatus::Unreachable
        | MemoryOperationStatus::Valid
        | MemoryOperationStatus::Recovered => None,
    }
}

impl<'analysis, C> StorageFlowCollector<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn transfer_memory_result_state(
        &self,
        state: &mut StorageFlowState,
        node: AnyBoundNodeId,
    ) {
        let AnyBoundNodeId::Expression(expression) = node else {
            return;
        };

        let Some(bound) = self.request.unit().tree().expression(expression) else {
            return;
        };

        if let Some(result) = self.expression_result_storage(expression) {
            let mut child_source = None;

            for child in bound.child_expressions() {
                let Some(storage) = self.expression_value_storage(state, child) else {
                    continue;
                };

                match child_source {
                    Some(previous) if previous != storage => {
                        child_source = None;
                        break;
                    }
                    Some(_) => {}
                    None => child_source = Some(storage),
                }
            }

            if let Some(source) = child_source {
                replace_raw_state(state, source, result);
            }
        }

        let Some(destination) = self.input.initialization_destination(expression) else {
            return;
        };

        let Some(source) = self.expression_value_storage(state, expression) else {
            return;
        };

        replace_raw_state(state, source, destination);
    }

    pub(super) fn transfer_raw_pointer_state(
        &self,
        state: &mut StorageFlowState,
        node: AnyBoundNodeId,
    ) {
        let mut sources = Vec::new();
        let mut destinations = Vec::new();

        for plan in self.input.plans(node) {
            let Some(root) = self.storage.root_identity(plan.access()) else {
                continue;
            };

            match self.effective_purpose(*plan) {
                StorageAccessPurpose::Move
                | StorageAccessPurpose::Copy
                | StorageAccessPurpose::ValueTransfer => sources.push((root, plan.purpose())),
                StorageAccessPurpose::Initialize | StorageAccessPurpose::Assignment => {
                    destinations.push(root);
                }
                StorageAccessPurpose::Read
                | StorageAccessPurpose::Write
                | StorageAccessPurpose::Borrow(_)
                | StorageAccessPurpose::Member
                | StorageAccessPurpose::Index
                | StorageAccessPurpose::Slice
                | StorageAccessPurpose::Projection => {}
            }
        }

        let ([(source, purpose)], [destination]) = (sources.as_slice(), destinations.as_slice())
        else {
            return;
        };

        replace_raw_state(state, *source, *destination);

        if *purpose == StorageAccessPurpose::Move {
            state.raw_initialized.remove(source);
            state.active_allocations.remove(source);
            state.invalidated_allocations.remove(source);
        }
    }

    pub(super) fn apply_memory_operation(
        &mut self,
        state: &mut StorageFlowState,
        node: AnyBoundNodeId,
        refinements: &[Refinement],
    ) {
        let AnyBoundNodeId::Expression(expression) = node else {
            return;
        };

        let Some(operation) = self.memory.operation(expression) else {
            return;
        };

        let status = if !state.reachable {
            MemoryOperationStatus::Unreachable
        } else if state.recovered {
            MemoryOperationStatus::Recovered
        } else if operation_requires_trust(operation.kind())
            && !refinements
                .iter()
                .any(|refinement| matches!(refinement.kind(), RefinementKind::TrustBoundary(_)))
        {
            MemoryOperationStatus::MissingTrustedEvidence
        } else {
            self.apply_valid_memory_operation(state, operation)
        };

        self.is_recovered |= status == MemoryOperationStatus::Recovered;

        if self.publish {
            self.memory_decisions
                .entry(expression)
                .and_modify(|current| *current = conservative_memory_status(*current, status))
                .or_insert(status);

            if let Some(kind) = memory_diagnostic_kind(status) {
                let origins = self.memory_failure_origins(state, operation, status);
                self.add_memory_diagnostic(kind, operation, origins);
            }
        }
    }

    fn apply_valid_memory_operation(
        &self,
        state: &mut StorageFlowState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
    ) -> MemoryOperationStatus {
        let arguments = operation.arguments();

        match operation.kind() {
            CheckedMemoryOperationKind::UninitNew { .. } => {}
            CheckedMemoryOperationKind::UninitPointer { .. } => {
                let Some(source) = arguments
                    .first()
                    .and_then(|argument| self.argument_storage(*argument))
                else {
                    return MemoryOperationStatus::Recovered;
                };

                let Some(result) = self.operation_result_storage(operation.expression()) else {
                    return MemoryOperationStatus::Recovered;
                };

                replace_raw_state(state, source, result);
            }
            CheckedMemoryOperationKind::UninitWrite { element } => {
                let Some(storage) = arguments
                    .first()
                    .and_then(|argument| self.argument_storage(*argument))
                else {
                    return MemoryOperationStatus::Recovered;
                };

                let status = apply_raw_write(state, storage, element, operation.expression());

                if status != MemoryOperationStatus::Valid {
                    return status;
                }

                if let Some(result) = self.operation_result_storage(operation.expression()) {
                    replace_raw_state(state, storage, result);
                }
            }
            CheckedMemoryOperationKind::UninitAssumeInitialized { element }
            | CheckedMemoryOperationKind::UninitMove { element } => {
                let Some(storage) = arguments
                    .first()
                    .and_then(|argument| self.argument_storage(*argument))
                else {
                    return MemoryOperationStatus::Recovered;
                };

                return consume_trusted_uninit(state, storage, element);
            }
            CheckedMemoryOperationKind::BorrowFrom { .. } => {
                let [_, pointer, ..] = arguments else {
                    return MemoryOperationStatus::Recovered;
                };

                let Some(source) = self.argument_storage(*pointer) else {
                    return MemoryOperationStatus::Recovered;
                };

                if let Some(result) = self.operation_result_storage(operation.expression()) {
                    replace_raw_state(state, source, result);
                }
            }
            CheckedMemoryOperationKind::Address { pointee, .. } => {
                let Some(result) = self.operation_result_storage(operation.expression()) else {
                    return MemoryOperationStatus::Recovered;
                };

                state
                    .raw_initialized
                    .entry(result)
                    .or_default()
                    .entry(pointee)
                    .or_default()
                    .insert(operation.expression());
            }
            CheckedMemoryOperationKind::Null { .. } => {}
            CheckedMemoryOperationKind::IsNull { .. }
            | CheckedMemoryOperationKind::LayoutQuery { .. } => {}
            CheckedMemoryOperationKind::Offset { .. }
            | CheckedMemoryOperationKind::Reinterpret { .. }
            | CheckedMemoryOperationKind::CallableFromPointer { .. }
            | CheckedMemoryOperationKind::PointerFromCallable { .. } => {
                let Some(source) = arguments
                    .first()
                    .and_then(|argument| self.argument_storage(*argument))
                else {
                    return MemoryOperationStatus::Recovered;
                };

                let Some(result) = self.operation_result_storage(operation.expression()) else {
                    return MemoryOperationStatus::Recovered;
                };

                replace_raw_state(state, source, result);
            }
            CheckedMemoryOperationKind::Read { pointee, kind }
            | CheckedMemoryOperationKind::VolatileRead { pointee, kind, .. } => {
                let Some(pointer) = arguments
                    .first()
                    .and_then(|argument| self.argument_storage(*argument))
                else {
                    return MemoryOperationStatus::Recovered;
                };

                return apply_raw_read(state, pointer, pointee, kind);
            }
            CheckedMemoryOperationKind::Write { pointee }
            | CheckedMemoryOperationKind::VolatileWrite { pointee, .. } => {
                let Some(pointer) = arguments
                    .first()
                    .and_then(|argument| self.argument_storage(*argument))
                else {
                    return MemoryOperationStatus::Recovered;
                };

                return apply_raw_write(state, pointer, pointee, operation.expression());
            }
            CheckedMemoryOperationKind::Copy { pointee, .. } => {
                let [source, destination, ..] = arguments else {
                    return MemoryOperationStatus::Recovered;
                };

                let Some(source) = self.argument_storage(*source) else {
                    return MemoryOperationStatus::Recovered;
                };

                let Some(destination) = self.argument_storage(*destination) else {
                    return MemoryOperationStatus::Recovered;
                };

                return apply_raw_copy(state, source, destination, pointee, operation.expression());
            }
            CheckedMemoryOperationKind::RawAllocate | CheckedMemoryOperationKind::Allocate => {
                let Some(result) = self.operation_result_storage(operation.expression()) else {
                    return MemoryOperationStatus::Recovered;
                };

                state
                    .active_allocations
                    .entry(result)
                    .or_default()
                    .insert(operation.expression());

                state
                    .allocation_origins
                    .entry(result)
                    .or_default()
                    .insert(operation.expression());

                state.invalidated_allocations.remove(&result);
            }
            CheckedMemoryOperationKind::RawDeallocate | CheckedMemoryOperationKind::Deallocate => {
                let Some(pointer) = arguments
                    .first()
                    .and_then(|argument| self.argument_storage(*argument))
                else {
                    return MemoryOperationStatus::Recovered;
                };

                return apply_deallocation(state, pointer, operation.expression());
            }
            CheckedMemoryOperationKind::ByteBufferFill
            | CheckedMemoryOperationKind::ByteSliceCopy
            | CheckedMemoryOperationKind::ByteBufferRead
            | CheckedMemoryOperationKind::SliceLength
            | CheckedMemoryOperationKind::RawBufferCapacity
            | CheckedMemoryOperationKind::RawBufferInitializedCount
            | CheckedMemoryOperationKind::RawBufferPointer
            | CheckedMemoryOperationKind::RawBufferInitializedSlice
            | CheckedMemoryOperationKind::RawBufferInitializedSliceMut
            | CheckedMemoryOperationKind::RawBufferSparePointer { .. }
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

    fn argument_storage(&self, expression: BoundExpressionId) -> Option<StorageIdentityId> {
        self.storage
            .access_plans()
            .iter()
            .filter(|plan| plan.expression() == expression)
            .find_map(|plan| self.storage.root_identity(plan.access()))
    }

    fn operation_result_storage(&self, expression: BoundExpressionId) -> Option<StorageIdentityId> {
        self.expression_result_storage(expression)
    }

    fn expression_result_storage(
        &self,
        expression: BoundExpressionId,
    ) -> Option<StorageIdentityId> {
        self.storage.identity_entries().find_map(|(id, identity)| {
            matches!(
                identity,
                StorageIdentity::Temporary(candidate) | StorageIdentity::Allocation(candidate)
                    if candidate == expression
            )
            .then_some(id)
        })
    }

    fn expression_value_storage(
        &self,
        state: &StorageFlowState,
        expression: BoundExpressionId,
    ) -> Option<StorageIdentityId> {
        self.expression_result_storage(expression)
            .filter(|storage| has_raw_state(state, *storage))
            .or_else(|| {
                self.storage
                    .access_plans()
                    .iter()
                    .filter(|plan| plan.expression() == expression)
                    .filter_map(|plan| self.storage.root_identity(plan.access()))
                    .find(|storage| has_raw_state(state, *storage))
            })
    }

    fn memory_failure_origins(
        &self,
        state: &StorageFlowState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        status: MemoryOperationStatus,
    ) -> Vec<(DiagnosticRelatedLocationKind, Vec<BoundExpressionId>)> {
        let storages = operation
            .arguments()
            .iter()
            .filter_map(|argument| self.argument_storage(*argument))
            .collect::<Vec<_>>();

        match status {
            MemoryOperationStatus::InvalidatedAllocation => vec![
                (
                    DiagnosticRelatedLocationKind::AllocationOrigin,
                    storages
                        .iter()
                        .flat_map(|storage| {
                            state
                                .allocation_origins
                                .get(storage)
                                .into_iter()
                                .flatten()
                                .copied()
                        })
                        .collect(),
                ),
                (
                    DiagnosticRelatedLocationKind::DeallocationOrigin,
                    storages
                        .iter()
                        .flat_map(|storage| {
                            state
                                .invalidated_allocations
                                .get(storage)
                                .into_iter()
                                .flatten()
                                .copied()
                        })
                        .collect(),
                ),
            ],
            MemoryOperationStatus::UninitializedRawStorage => vec![(
                DiagnosticRelatedLocationKind::AllocationOrigin,
                storages
                    .iter()
                    .flat_map(|storage| {
                        state
                            .allocation_origins
                            .get(storage)
                            .into_iter()
                            .flatten()
                            .copied()
                    })
                    .collect(),
            )],
            MemoryOperationStatus::OutstandingObligations => vec![(
                DiagnosticRelatedLocationKind::InitializationOrigin,
                storages
                    .iter()
                    .flat_map(|storage| {
                        state
                            .raw_initialized
                            .get(storage)
                            .into_iter()
                            .flat_map(BTreeMap::values)
                            .flatten()
                            .copied()
                    })
                    .collect(),
            )],
            MemoryOperationStatus::Unreachable
            | MemoryOperationStatus::Valid
            | MemoryOperationStatus::Recovered
            | MemoryOperationStatus::MissingTrustedEvidence => Vec::new(),
        }
    }

    fn add_memory_diagnostic(
        &mut self,
        kind: DiagnosticKind,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        origins: Vec<(DiagnosticRelatedLocationKind, Vec<BoundExpressionId>)>,
    ) {
        let expression = operation.expression();

        if !self.reported_memory_diagnostics.insert((kind, expression)) {
            return;
        }

        let Some(node) = self.request.view().expression(expression) else {
            self.is_recovered = true;

            return;
        };

        let span = match self.request.source(node.origin().source_anchor()) {
            Ok(source) => source.span(),
            Err(_) => {
                self.is_recovered = true;

                return;
            }
        };

        let mut diagnostic = Diagnostic::new(
            DiagnosticId::from_index(self.diagnostics.len()),
            kind,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::MemoryOperationFailure,
            span,
        ))
        .with_arg(DiagnosticArg::memory_operation(
            crate::memory_diagnostics::diagnostic_checked_memory_operation(operation.kind()),
        ));

        for (kind, origins) in origins {
            for origin in origins
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
            {
                let Some(node) = self.request.view().expression(origin) else {
                    continue;
                };

                let Ok(source) = self.request.source(node.origin().source_anchor()) else {
                    continue;
                };

                if source.span() != span {
                    diagnostic = diagnostic
                        .with_related_location(DiagnosticRelatedLocation::new(kind, source.span()));
                }
            }
        }

        self.diagnostics.add(diagnostic);
    }

    pub(super) fn memory_decisions(&self) -> impl Iterator<Item = MemoryOperationDecision> + '_ {
        self.memory_decisions
            .iter()
            .map(|(expression, status)| MemoryOperationDecision::new(*expression, *status))
    }
}

fn apply_raw_read(
    state: &mut StorageFlowState,
    pointer: StorageIdentityId,
    pointee: bray_symbols::TypeId,
    kind: bray_bound_tree::MemoryReadKind,
) -> MemoryOperationStatus {
    if state.invalidated_allocations.contains_key(&pointer) {
        return MemoryOperationStatus::InvalidatedAllocation;
    }

    let Some(initialized) = state.raw_initialized.get_mut(&pointer) else {
        return MemoryOperationStatus::UninitializedRawStorage;
    };

    if !initialized.contains_key(&pointee) {
        return MemoryOperationStatus::UninitializedRawStorage;
    }

    if kind == bray_bound_tree::MemoryReadKind::Move {
        initialized.remove(&pointee);
    }

    MemoryOperationStatus::Valid
}

fn consume_trusted_uninit(
    state: &mut StorageFlowState,
    storage: StorageIdentityId,
    element: bray_symbols::TypeId,
) -> MemoryOperationStatus {
    if state.invalidated_allocations.contains_key(&storage) {
        return MemoryOperationStatus::InvalidatedAllocation;
    }

    if let Some(initialized) = state.raw_initialized.get_mut(&storage) {
        initialized.remove(&element);
    }

    MemoryOperationStatus::Valid
}

fn apply_raw_write(
    state: &mut StorageFlowState,
    pointer: StorageIdentityId,
    pointee: bray_symbols::TypeId,
    origin: BoundExpressionId,
) -> MemoryOperationStatus {
    if state.invalidated_allocations.contains_key(&pointer) {
        return MemoryOperationStatus::InvalidatedAllocation;
    }

    state
        .raw_initialized
        .entry(pointer)
        .or_default()
        .entry(pointee)
        .or_default()
        .insert(origin);

    MemoryOperationStatus::Valid
}

fn apply_raw_copy(
    state: &mut StorageFlowState,
    source: StorageIdentityId,
    destination: StorageIdentityId,
    pointee: bray_symbols::TypeId,
    origin: BoundExpressionId,
) -> MemoryOperationStatus {
    if state.invalidated_allocations.contains_key(&source)
        || state.invalidated_allocations.contains_key(&destination)
    {
        return MemoryOperationStatus::InvalidatedAllocation;
    }

    if !state
        .raw_initialized
        .get(&source)
        .is_some_and(|initialized| initialized.contains_key(&pointee))
    {
        return MemoryOperationStatus::UninitializedRawStorage;
    }

    state
        .raw_initialized
        .entry(destination)
        .or_default()
        .entry(pointee)
        .or_default()
        .insert(origin);

    MemoryOperationStatus::Valid
}

fn apply_deallocation(
    state: &mut StorageFlowState,
    pointer: StorageIdentityId,
    origin: BoundExpressionId,
) -> MemoryOperationStatus {
    if state.invalidated_allocations.contains_key(&pointer) {
        return MemoryOperationStatus::InvalidatedAllocation;
    }

    if state
        .raw_initialized
        .get(&pointer)
        .is_some_and(|initialized| !initialized.is_empty())
    {
        return MemoryOperationStatus::OutstandingObligations;
    }

    state.active_allocations.remove(&pointer);
    state.raw_initialized.remove(&pointer);

    state
        .invalidated_allocations
        .entry(pointer)
        .or_default()
        .insert(origin);

    MemoryOperationStatus::Valid
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundErrorExpression, BoundExpression, BoundExpressionId, BoundUnitId, BoundUnitKind,
        MemoryOperationStatus, MemoryReadKind, StorageIdentity, StorageIdentityId,
        StoragePlanBuilder,
    };

    use super::{
        StorageFlowState, apply_deallocation, apply_raw_copy, apply_raw_read, apply_raw_write,
        consume_trusted_uninit, replace_raw_state,
    };
    use crate::test_support::{error_type, expression_unit, push_expression};

    fn raw_storage_pair(
        unit: BoundUnitId,
    ) -> ([BoundExpressionId; 2], StorageIdentityId, StorageIdentityId) {
        let (_, expressions) = expression_unit(unit, |tree, origin| {
            (0..2)
                .map(|_| {
                    push_expression(
                        tree,
                        BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
                    )
                })
                .collect::<Vec<_>>()
        });

        let [source_expression, destination_expression] = expressions.as_slice() else {
            panic!("raw storage fixture must contain exactly two expressions");
        };

        let mut storage = StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);

        let source = storage
            .push_identity(StorageIdentity::Temporary(*source_expression))
            .unwrap_or_else(|error| panic!("source storage must validate: {error:?}"));

        let destination = storage
            .push_identity(StorageIdentity::Temporary(*destination_expression))
            .unwrap_or_else(|error| panic!("destination storage must validate: {error:?}"));

        (
            [*source_expression, *destination_expression],
            source,
            destination,
        )
    }

    #[test]
    fn trusted_uninit_consumption_uses_the_checked_predicate_as_authority() {
        let (expressions, storage, _) = raw_storage_pair(BoundUnitId::new(82));

        let element = error_type();

        let mut state = StorageFlowState {
            reachable: true,
            ..StorageFlowState::default()
        };

        assert_eq!(
            consume_trusted_uninit(&mut state, storage, element),
            MemoryOperationStatus::Valid
        );

        state
            .raw_initialized
            .entry(storage)
            .or_default()
            .entry(element)
            .or_default()
            .insert(expressions[0]);

        assert_eq!(
            consume_trusted_uninit(&mut state, storage, element),
            MemoryOperationStatus::Valid
        );

        assert!(!state.raw_initialized[&storage].contains_key(&element));
    }

    #[test]
    fn raw_memory_transitions_preserve_initialization_and_invalidation() {
        let unit = BoundUnitId::new(41);

        let (expressions, source, destination) = raw_storage_pair(unit);

        let ty = error_type();
        let mut state = StorageFlowState::default();

        assert_eq!(
            apply_raw_write(&mut state, source, ty, expressions[0]),
            MemoryOperationStatus::Valid
        );

        assert_eq!(
            apply_raw_copy(&mut state, source, destination, ty, expressions[1]),
            MemoryOperationStatus::Valid
        );

        assert_eq!(
            apply_raw_read(&mut state, source, ty, MemoryReadKind::Copy),
            MemoryOperationStatus::Valid
        );

        assert_eq!(
            apply_raw_read(&mut state, source, ty, MemoryReadKind::Move),
            MemoryOperationStatus::Valid
        );

        assert_eq!(
            apply_raw_read(&mut state, source, ty, MemoryReadKind::Move),
            MemoryOperationStatus::UninitializedRawStorage
        );

        assert_eq!(
            apply_deallocation(&mut state, destination, expressions[1]),
            MemoryOperationStatus::OutstandingObligations
        );

        state.raw_initialized.remove(&destination);

        assert_eq!(
            apply_deallocation(&mut state, destination, expressions[1]),
            MemoryOperationStatus::Valid
        );

        assert_eq!(
            apply_raw_write(&mut state, destination, ty, expressions[1]),
            MemoryOperationStatus::InvalidatedAllocation
        );
    }

    #[test]
    fn raw_pointer_reassignment_clears_state_absent_from_the_new_pointer() {
        let unit = BoundUnitId::new(42);

        let (expressions, source, destination) = raw_storage_pair(unit);

        let ty = error_type();
        let origin = expressions[1];
        let mut state = StorageFlowState::default();

        state
            .raw_initialized
            .entry(destination)
            .or_default()
            .entry(ty)
            .or_default()
            .insert(origin);

        state
            .active_allocations
            .entry(destination)
            .or_default()
            .insert(origin);

        state
            .allocation_origins
            .entry(destination)
            .or_default()
            .insert(origin);

        state
            .invalidated_allocations
            .entry(destination)
            .or_default()
            .insert(origin);

        replace_raw_state(&mut state, source, destination);

        assert!(!state.raw_initialized.contains_key(&destination));
        assert!(!state.active_allocations.contains_key(&destination));
        assert!(!state.allocation_origins.contains_key(&destination));
        assert!(!state.invalidated_allocations.contains_key(&destination));
    }
}
