use crate::analysis::storage_flow::memory::Effect;
use bray_bound_tree::{
    AnyBoundNodeId, BoundExpressionId, CheckedMemoryOperationKind, MemoryOperationDecision,
    MemoryOperationStatus, Refinement, RefinementKind, StorageAccessPurpose, StorageIdentity,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, SeverityKind,
};

use super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Place, Source, Target};
use crate::analysis::storage_flow::model::StorageFlowState;

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
    pub(super) fn apply_memory_operation<'refinements>(
        &mut self,
        state: &mut StorageFlowState,
        node: AnyBoundNodeId,
        refinements: impl Iterator<Item = &'refinements Refinement> + Clone,
    ) -> Option<MemoryOperationStatus> {
        let AnyBoundNodeId::Expression(expression) = node else {
            return None;
        };

        let Some(operation) = self.memory.operation(expression) else {
            return None;
        };

        let proven = self.memory_evidence;

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

                        self.expression_target(state, argument)
                            .map(|place| (state.memory.reached_storage(&place), element, purpose))
                    });

            let status = self.apply_valid_memory_operation(state, operation, proven);

            if status == MemoryOperationStatus::Valid && proven != Some(true) {
                if let Some((targets, element, purpose)) = targets {
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
            }

            status
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
            | CheckedMemoryOperationKind::Reinterpret { .. }
            | CheckedMemoryOperationKind::CallableFromPointer { .. }
            | CheckedMemoryOperationKind::PointerFromCallable { .. } => {
                let pointer = usize::from(matches!(operation.kind(), CheckedMemoryOperationKind::BorrowFrom { .. }));

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

                state.memory.fresh(result, operation.expression());
            }
            CheckedMemoryOperationKind::RawDeallocate | CheckedMemoryOperationKind::Deallocate => {
                let Some(pointer) = argument(0) else {
                    return MemoryOperationStatus::Recovered;
                };

                return state.memory.deallocate(&pointer, operation.expression());
            }
            CheckedMemoryOperationKind::RawBufferSparePointer { .. }
            | CheckedMemoryOperationKind::RawBufferInitializedSlice
            | CheckedMemoryOperationKind::RawBufferInitializedSliceMut => {
                let Some(source) = self
                    .expression_reference(state, arguments[0])
                    .and_then(|source| source.targets(&state.memory))
                else {
                    return MemoryOperationStatus::Recovered;
                };

                if let ([owner], Some(result)) = (source.as_slice(), result()) {
                    // This closed intrinsic exposes the owned allocation, not the owner's bytes.
                    let ty = match operation.kind() {
                        CheckedMemoryOperationKind::RawBufferSparePointer { element } => self
                            .request
                            .semantic_values()
                            .intern_type(bray_symbols::TypeData::Slice(element))
                            .expect("checked buffer element has a slice type"),
                        _ => self.request.semantic_values().unborrowed_type(
                            self.execution
                                .semantics
                                .types()
                                .expression(operation.expression())
                                .expect("checked buffer view has its result type")
                                .ty(),
                        ),
                    };

                    state.memory.owned_region(owner.clone(), result.clone(), ty);

                    if matches!(
                        operation.kind(),
                        CheckedMemoryOperationKind::RawBufferSparePointer { .. }
                    ) {
                        // Its initialized prefix has an unknown offset into this allocation.
                        state
                            .memory
                            .offset(&Target::storage(result.clone()), result);
                    }
                }
            }
            CheckedMemoryOperationKind::ByteBufferFill
            | CheckedMemoryOperationKind::ByteBufferRead
            | CheckedMemoryOperationKind::SequenceLength
            | CheckedMemoryOperationKind::RawBufferCapacity
            | CheckedMemoryOperationKind::RawBufferInitializedCount
            | CheckedMemoryOperationKind::RawBufferPointer
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
                    state.memory.point_at(payload, result, element, bray_symbols::DependencyReferenceKind::Address);
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
                    state.memory.point_at(payload, result, element, bray_symbols::DependencyReferenceKind::Borrow);
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

    fn memory_mutation(
        &self,
        operation: &bray_bound_tree::CheckedMemoryOperation,
    ) -> Option<(
        BoundExpressionId,
        bray_symbols::TypeId,
        StorageAccessPurpose,
    )> {
        use bray_bound_tree::MemoryReadKind;

        let (index, element, purpose) = match operation.kind() {
            CheckedMemoryOperationKind::Read {
                pointee: element,
                kind: MemoryReadKind::Move,
            }
            | CheckedMemoryOperationKind::VolatileRead {
                pointee: element,
                kind: MemoryReadKind::Move,
                ..
            }
            | CheckedMemoryOperationKind::UninitMove { element }
            | CheckedMemoryOperationKind::UninitAssumeInitialized { element } => {
                (0, element, StorageAccessPurpose::Move)
            }
            CheckedMemoryOperationKind::Write { pointee: element }
            | CheckedMemoryOperationKind::VolatileWrite {
                pointee: element, ..
            }
            | CheckedMemoryOperationKind::UninitWrite { element } => {
                (0, element, StorageAccessPurpose::Initialize)
            }
            CheckedMemoryOperationKind::Copy { pointee, .. } => {
                (1, pointee, StorageAccessPurpose::Initialize)
            }
            _ => return None,
        };

        operation
            .arguments()
            .get(index)
            .copied()
            .map(|argument| (argument, element, purpose))
    }

    pub(super) fn assignment_target(
        &self,
        state: &StorageFlowState,
        node: AnyBoundNodeId,
    ) -> Option<Target> {
        let access = self
            .storage
            .node_plans(node)
            .find(|plan| plan.purpose() == StorageAccessPurpose::Assignment)?
            .access();

        self.input
            .reference_target(&state.memory, self.storage, access)
    }

    pub(super) fn memory_effects(
        &self,
        state: &StorageFlowState,
        expression: BoundExpressionId,
    ) -> Option<Vec<Effect>> {
        let operation = self.memory.operation(expression)?;

        if crate::memory::execution::storage_effect(operation.kind())
            == crate::memory::execution::StorageEffect::None
        {
            return Some(Vec::new());
        }

        if matches!(
            operation.kind(),
            CheckedMemoryOperationKind::AtomicStore { .. }
                | CheckedMemoryOperationKind::AtomicExchange { .. }
                | CheckedMemoryOperationKind::AtomicCompareExchange { .. }
                | CheckedMemoryOperationKind::AtomicFetch { .. }
        ) {
            // Atomics update their protected word, never retire its owner or unrelated storage.
            let targets = self.expression_reference(state, operation.arguments()[0])
                .and_then(|source| source.targets(&state.memory));

            let Some(targets) = targets else { return Some(vec![Effect::Atomic]); };

            return Some(targets.into_iter()
                .map(|target| Effect::Mutation(target, bray_symbols::DependencyMutationKind::Atomic))
                .collect());
        }

        let (argument, _, purpose) = self.memory_mutation(operation)?;

        let targets = if matches!(
            operation.kind(),
            CheckedMemoryOperationKind::UninitMove { .. }
                | CheckedMemoryOperationKind::UninitAssumeInitialized { .. }
                | CheckedMemoryOperationKind::UninitWrite { .. }
        ) {
            vec![self.uninit_target(state, argument)?]
        } else {
            let slot = self.argument_storage(argument)?;

            state.memory.referenced_regions(&Target::storage(slot))?
        };

        let kind = if purpose == StorageAccessPurpose::Move {
            bray_symbols::DependencyMutationKind::Consume
        } else {
            bray_symbols::DependencyMutationKind::Write
        };

        Some(targets.into_iter().map(|target| Effect::Mutation(target, kind)).collect())
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn expression_is_borrowed(
        &self,
        expression: BoundExpressionId,
    ) -> bool {
        self.execution.semantics.types().expression(expression).is_some_and(|ty| {
            matches!(
                self.request.semantic_values().type_data(ty.ty()).as_ref(),
                bray_symbols::TypeData::Borrow { .. }
            )
        })
    }

    pub(super) fn uninit_target(
        &self,
        state: &StorageFlowState,
        expression: BoundExpressionId,
    ) -> Option<Target> {
        let borrowed = self.expression_is_borrowed(expression);

        let storage = if borrowed {
            let targets = self
                .expression_reference(state, expression)?
                .targets(&state.memory)?;

            if targets.len() != 1 {
                return None;
            }

            targets
                .into_iter()
                .next()
                .expect("one protected storage target")
        } else {
            self.expression_target(state, expression)?
        };

        Some(storage.project(&[bray_bound_tree::StorageProjection::UninitPayload]))
    }

    pub(super) fn expression_target(
        &self,
        state: &StorageFlowState,
        expression: BoundExpressionId,
    ) -> Option<Target> {
        self.expression_result_storage(expression)
            .map(Target::storage)
            .or_else(|| {
                self.storage.node_plans(expression.into()).find_map(|plan| {
                    self.input
                        .reference_target(&state.memory, self.storage, plan.access())
                })
            })
            .or_else(|| self.argument_storage(expression).map(Target::storage))
    }

    pub(super) fn argument_storage(&self, expression: BoundExpressionId) -> Option<Place> {
        self.input
            .expression_places(expression)
            .first()
            .cloned()
            .or_else(|| self.expression_result_storage(expression))
    }

    pub(super) fn expression_result_storage(&self, expression: BoundExpressionId) -> Option<Place> {
        self.input.definitions(expression.into()).iter().copied().find(|identity| {
            matches!(self.storage.identity(*identity), Some(StorageIdentity::Temporary(candidate) | StorageIdentity::Allocation(candidate)) if candidate == expression)
        }).map(Place::root)
    }

    pub(super) fn expression_reference(
        &self,
        state: &StorageFlowState,
        expression: BoundExpressionId,
    ) -> Option<Source> {
        if let Some(slot) = self
            .expression_result_storage(expression)
            .filter(|storage| state.memory.has_stored_value(storage))
        {
            return Some(Source::Value(Target::storage(slot)));
        }

        let borrowed = self.expression_is_borrowed(expression);

        if borrowed
            && let Some(capability) = self
                .storage
                .node_plans(expression.into())
                .filter_map(|plan| self.storage.borrow_for_plan(plan))
                .find(|capability| state.memory.has_borrow(*capability))
        {
            // A newly formed borrow names the aggregate, not a reference stored inside it.
            return Some(Source::Borrow(capability));
        }

        if let Some(target) = self.storage.node_plans(expression.into()).find_map(|plan| {
            let target = self
                .input
                .reference_target(&state.memory, self.storage, plan.access())?;

            state.memory.has_value(&target).then_some(target)
        }) {
            return Some(Source::Value(target));
        }

        if let Some(slot) = self
            .argument_storage(expression)
            .filter(|storage| state.memory.has_stored_value(storage))
        {
            return Some(Source::Value(Target::storage(slot)));
        }

        if !borrowed {
            return None;
        }

        self.storage
            .node_plans(expression.into())
            .filter_map(|plan| {
                self.storage
                    .access(plan.access())?
                    .root()
                    .borrow_capability()
            })
            .find(|capability| state.memory.has_borrow(*capability))
            .map(Source::Borrow)
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
            .filter_map(|argument| self.expression_target(state, *argument))
            .collect::<Vec<_>>();

        match status {
            MemoryOperationStatus::InvalidatedAllocation => vec![
                (
                    DiagnosticRelatedLocationKind::AllocationOrigin,
                    storages
                        .iter()
                        .flat_map(|storage| state.memory.allocation_origins(storage))
                        .collect(),
                ),
                (
                    DiagnosticRelatedLocationKind::DeallocationOrigin,
                    storages
                        .iter()
                        .flat_map(|storage| state.memory.invalidation_origins(storage))
                        .collect(),
                ),
            ],
            MemoryOperationStatus::UninitializedRawStorage => vec![(
                DiagnosticRelatedLocationKind::AllocationOrigin,
                storages
                    .iter()
                    .flat_map(|storage| state.memory.allocation_origins(storage))
                    .collect(),
            )],
            MemoryOperationStatus::OutstandingObligations => vec![(
                DiagnosticRelatedLocationKind::InitializationOrigin,
                storages
                    .iter()
                    .flat_map(|storage| state.memory.initialization_origins(storage))
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

        let span = self.request.source(node.origin().source_anchor()).span();

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

                let source = self.request.source(node.origin().source_anchor());

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
