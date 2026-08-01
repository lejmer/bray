use bray_bound_tree::{
    AnyBoundNodeId, BoundExpressionId, CheckedMemoryOperationKind, MemoryOperationDecision,
    MemoryOperationStatus, RefinementFact, RefinementFactKind, StorageAccessPurpose,
    StorageIdentity, StorageIdentityId,
};
use bray_diagnostics::{Diagnostic, DiagnosticId, DiagnosticKind, SeverityKind};

use super::core::StorageFlowCollector;
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::CheckerRequestContext;

const fn operation_requires_trust(kind: CheckedMemoryOperationKind) -> bool {
    matches!(
        kind,
        CheckedMemoryOperationKind::Reinterpret { .. }
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
            | CheckedMemoryOperationKind::ByteBufferFill
            | CheckedMemoryOperationKind::ByteBufferCopy
            | CheckedMemoryOperationKind::ByteBufferRead
    )
}

fn copy_raw_state(
    state: &mut StorageFlowState,
    source: StorageIdentityId,
    destination: StorageIdentityId,
) {
    if let Some(initialized) = state.raw_initialized.get(&source).cloned() {
        state.raw_initialized.insert(destination, initialized);
    }

    if state.active_allocations.contains(&source) {
        state.active_allocations.insert(destination);
    }

    if state.invalidated_allocations.contains(&source) {
        state.invalidated_allocations.insert(destination);
    }
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
        MemoryOperationStatus::MissingTrustedFacts => 3,
        MemoryOperationStatus::InvalidatedAllocation => 4,
        MemoryOperationStatus::UninitializedRawStorage => 5,
        MemoryOperationStatus::OutstandingObligations => 6,
    }
}

const fn memory_diagnostic_kind(status: MemoryOperationStatus) -> Option<DiagnosticKind> {
    match status {
        MemoryOperationStatus::MissingTrustedFacts => {
            Some(DiagnosticKind::CheckingMissingTrustedMemoryFacts)
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

        copy_raw_state(state, *source, *destination);

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
        refinements: &[RefinementFact],
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
                .any(|fact| matches!(fact.kind(), RefinementFactKind::TrustBoundary(_)))
        {
            MemoryOperationStatus::MissingTrustedFacts
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
                self.add_memory_diagnostic(kind, expression);
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
            CheckedMemoryOperationKind::Address { pointee, .. } => {
                let Some(result) = self.operation_result_storage(operation.expression()) else {
                    return MemoryOperationStatus::Recovered;
                };

                state
                    .raw_initialized
                    .entry(result)
                    .or_default()
                    .insert(pointee);
            }
            CheckedMemoryOperationKind::Null { .. } => {}
            CheckedMemoryOperationKind::IsNull { .. }
            | CheckedMemoryOperationKind::LayoutQuery { .. } => {}
            CheckedMemoryOperationKind::Offset { .. }
            | CheckedMemoryOperationKind::Reinterpret { .. } => {
                let Some(source) = arguments
                    .first()
                    .and_then(|argument| self.argument_storage(*argument))
                else {
                    return MemoryOperationStatus::Recovered;
                };

                let Some(result) = self.operation_result_storage(operation.expression()) else {
                    return MemoryOperationStatus::Recovered;
                };

                copy_raw_state(state, source, result);
            }
            CheckedMemoryOperationKind::Read { pointee, kind } => {
                let Some(pointer) = arguments
                    .first()
                    .and_then(|argument| self.argument_storage(*argument))
                else {
                    return MemoryOperationStatus::Recovered;
                };

                return apply_raw_read(state, pointer, pointee, kind);
            }
            CheckedMemoryOperationKind::Write { pointee } => {
                let Some(pointer) = arguments
                    .first()
                    .and_then(|argument| self.argument_storage(*argument))
                else {
                    return MemoryOperationStatus::Recovered;
                };

                return apply_raw_write(state, pointer, pointee);
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

                return apply_raw_copy(state, source, destination, pointee);
            }
            CheckedMemoryOperationKind::RawAllocate | CheckedMemoryOperationKind::Allocate => {
                let Some(result) = self.operation_result_storage(operation.expression()) else {
                    return MemoryOperationStatus::Recovered;
                };

                state.active_allocations.insert(result);
                state.invalidated_allocations.remove(&result);
            }
            CheckedMemoryOperationKind::RawDeallocate | CheckedMemoryOperationKind::Deallocate => {
                let Some(pointer) = arguments
                    .first()
                    .and_then(|argument| self.argument_storage(*argument))
                else {
                    return MemoryOperationStatus::Recovered;
                };

                return apply_deallocation(state, pointer);
            }
            CheckedMemoryOperationKind::ByteBufferFill
            | CheckedMemoryOperationKind::ByteBufferCopy
            | CheckedMemoryOperationKind::ByteBufferRead
            | CheckedMemoryOperationKind::ByteSliceLength
            | CheckedMemoryOperationKind::RawBufferCapacity
            | CheckedMemoryOperationKind::RawBufferInitializedCount
            | CheckedMemoryOperationKind::RawBufferPointer
            | CheckedMemoryOperationKind::RawBufferInitializedSlice
            | CheckedMemoryOperationKind::RawBufferInitializedSliceMut
            | CheckedMemoryOperationKind::RawBufferSparePointer { .. }
            | CheckedMemoryOperationKind::RawBufferSetInitializedCount
            | CheckedMemoryOperationKind::RawBufferRelease { .. } => {}
            | CheckedMemoryOperationKind::RawBufferReplace { .. } => {}
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
        self.storage.identity_entries().find_map(|(id, identity)| {
            matches!(
                identity,
                StorageIdentity::Temporary(candidate) | StorageIdentity::Allocation(candidate)
                    if candidate == expression
            )
            .then_some(id)
        })
    }

    fn add_memory_diagnostic(&mut self, kind: DiagnosticKind, expression: BoundExpressionId) {
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

        self.diagnostics.add(
            Diagnostic::new(
                DiagnosticId::from_index(self.diagnostics.len()),
                kind,
                SeverityKind::Error,
            )
            .with_primary_span(span),
        );
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
    if state.invalidated_allocations.contains(&pointer) {
        return MemoryOperationStatus::InvalidatedAllocation;
    }

    let Some(initialized) = state.raw_initialized.get_mut(&pointer) else {
        return MemoryOperationStatus::UninitializedRawStorage;
    };

    if !initialized.contains(&pointee) {
        return MemoryOperationStatus::UninitializedRawStorage;
    }

    if kind == bray_bound_tree::MemoryReadKind::Move {
        initialized.remove(&pointee);
    }

    MemoryOperationStatus::Valid
}

fn apply_raw_write(
    state: &mut StorageFlowState,
    pointer: StorageIdentityId,
    pointee: bray_symbols::TypeId,
) -> MemoryOperationStatus {
    if state.invalidated_allocations.contains(&pointer) {
        return MemoryOperationStatus::InvalidatedAllocation;
    }

    state
        .raw_initialized
        .entry(pointer)
        .or_default()
        .insert(pointee);

    MemoryOperationStatus::Valid
}

fn apply_raw_copy(
    state: &mut StorageFlowState,
    source: StorageIdentityId,
    destination: StorageIdentityId,
    pointee: bray_symbols::TypeId,
) -> MemoryOperationStatus {
    if state.invalidated_allocations.contains(&source)
        || state.invalidated_allocations.contains(&destination)
    {
        return MemoryOperationStatus::InvalidatedAllocation;
    }

    if !state
        .raw_initialized
        .get(&source)
        .is_some_and(|initialized| initialized.contains(&pointee))
    {
        return MemoryOperationStatus::UninitializedRawStorage;
    }

    state
        .raw_initialized
        .entry(destination)
        .or_default()
        .insert(pointee);

    MemoryOperationStatus::Valid
}

fn apply_deallocation(
    state: &mut StorageFlowState,
    pointer: StorageIdentityId,
) -> MemoryOperationStatus {
    if state.invalidated_allocations.contains(&pointer) {
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
    state.invalidated_allocations.insert(pointer);

    MemoryOperationStatus::Valid
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundErrorExpression, BoundExpression, BoundUnitId, BoundUnitKind, MemoryOperationStatus,
        MemoryReadKind, StorageIdentity, StoragePlanBuilder,
    };

    use super::{
        StorageFlowState, apply_deallocation, apply_raw_copy, apply_raw_read, apply_raw_write,
    };
    use crate::test_support::{error_type, expression_unit, push_expression};

    #[test]
    fn raw_memory_transitions_preserve_initialization_and_invalidation() {
        let unit = BoundUnitId::new(41);

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

        let mut storage = StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);

        let source = storage
            .push_identity(StorageIdentity::Temporary(expressions[0]))
            .unwrap_or_else(|error| panic!("source storage must validate: {error:?}"));

        let destination = storage
            .push_identity(StorageIdentity::Temporary(expressions[1]))
            .unwrap_or_else(|error| panic!("destination storage must validate: {error:?}"));

        let ty = error_type();
        let mut state = StorageFlowState::default();

        assert_eq!(
            apply_raw_write(&mut state, source, ty),
            MemoryOperationStatus::Valid
        );

        assert_eq!(
            apply_raw_copy(&mut state, source, destination, ty),
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
            apply_deallocation(&mut state, destination),
            MemoryOperationStatus::OutstandingObligations
        );

        state.raw_initialized.remove(&destination);

        assert_eq!(
            apply_deallocation(&mut state, destination),
            MemoryOperationStatus::Valid
        );

        assert_eq!(
            apply_raw_write(&mut state, destination, ty),
            MemoryOperationStatus::InvalidatedAllocation
        );
    }
}
