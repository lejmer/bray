use super::super::core::StorageFlowCollector;
use super::super::diagnostics::DiagnosticSubject;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::Effect;
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::{BoundExecutionSite, MemoryOperationStatus, StorageValueBinding};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticMemoryOperation, DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, SeverityKind,
};

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

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    fn memory_failure_origins(
        &self,
        state: &StorageFlowState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        status: MemoryOperationStatus,
        pointer: Option<&StorageValueBinding>,
    ) -> Vec<(DiagnosticRelatedLocationKind, Vec<BoundExecutionSite>)> {
        if status == MemoryOperationStatus::OutstandingObligations {
            return vec![(
                DiagnosticRelatedLocationKind::InitializationOrigin,
                state.memory.deallocation_duty_origins(pointer).into_iter().map(Into::into).collect(),
            )];
        }

        if !matches!(
            status,
            MemoryOperationStatus::InvalidatedAllocation | MemoryOperationStatus::UninitializedRawStorage
        ) {
            return Vec::new();
        }

        let execution = state.execution.as_ref().expect("memory diagnostics retain the evaluated inputs");

        let reached = || {
            pointer.into_iter().flat_map(|pointer| state.memory.captured_referent_contents(pointer))
                .chain(operation.arguments().iter().filter(|_| pointer.is_none())
                    .filter_map(|argument| self.captured_memory_operand(execution, operation, *argument)?.binding())
                    .flat_map(|pointer| state.memory.captured_referent_contents(pointer)))
        };

        let mut origins = vec![(
            DiagnosticRelatedLocationKind::AllocationOrigin,
            reached().flat_map(|contents| contents.allocations.iter().copied()).map(Into::into).collect(),
        )];

        if status == MemoryOperationStatus::InvalidatedAllocation {
            origins.push((
                DiagnosticRelatedLocationKind::DeallocationOrigin,
                reached().flat_map(|contents| contents.invalidations.iter().copied()).collect(),
            ));
        }

        origins
    }

    pub(super) fn add_memory_diagnostic(
        &mut self,
        state: &StorageFlowState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        status: MemoryOperationStatus,
        pointer: Option<&StorageValueBinding>,
    ) {
        let Some(kind) = memory_diagnostic_kind(status) else {
            return;
        };

        let origins = self.memory_failure_origins(state, operation, status, pointer);

        self.record_memory_diagnostic(operation.expression().into(), kind,
            crate::memory_diagnostics::diagnostic_checked_memory_operation(operation.kind()), origins);
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn check_raw_retirements(
        &mut self,
        state: &StorageFlowState,
        occurrence: BoundExecutionSite,
        effects: &[Effect],
    ) {
        use bray_symbols::{DependencyMutationKind as Mutation, DependencyReferenceKind};

        if !self.publish { return; }

        let mut origins = Vec::new();
        let mut failed = false;

        for effect in effects {
            let Effect::Mutation { address: Some(address), discharged, kind: Mutation::Retire, access_type: Some(ty), .. } = effect else { continue; };

            if crate::representation::reference_kind(self.request, *ty) != Some(DependencyReferenceKind::Address) {
                continue;
            }

            if let Some(pending) = state.memory.retirement_obligations(address, discharged.as_deref().unwrap_or_default()) {
                failed = true;
                origins.extend(pending.into_iter().map(BoundExecutionSite::from));
            }
        }

        if failed {
            self.record_memory_diagnostic(occurrence, DiagnosticKind::CheckingDeallocationWithOutstandingObligations,
                DiagnosticMemoryOperation::RawDeallocation,
                [(DiagnosticRelatedLocationKind::InitializationOrigin, origins)]);
        }
    }

    fn record_memory_diagnostic(
        &mut self,
        occurrence: BoundExecutionSite,
        kind: DiagnosticKind,
        operation: DiagnosticMemoryOperation,
        origins: impl IntoIterator<Item = (DiagnosticRelatedLocationKind, Vec<BoundExecutionSite>)>,
    ) {
        let node = crate::diagnostic::bound_node_origin(self.request, occurrence.node())
            .unwrap_or_else(|| panic!("checked memory transition {occurrence:?} retains its bound source"));

        let span = self.request.source(node.source_anchor()).span();

        let mut diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            kind,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::MemoryOperationFailure,
            span,
        ))
        .with_arg(DiagnosticArg::memory_operation(operation));

        for (kind, origins) in origins {
            for origin in origins
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
            {
                let node = crate::diagnostic::bound_node_origin(self.request, origin.node()).unwrap_or_else(|| {
                    panic!("memory failure origin {origin:?} retains its bound source")
                });

                let source = self.request.source(node.source_anchor());

                if source.span() != span {
                    diagnostic = diagnostic
                        .with_related_location(DiagnosticRelatedLocation::new(kind, source.span()));
                }
            }
        }

        self.diagnostics.record(DiagnosticSubject::Memory(kind, occurrence), diagnostic);
    }
}
