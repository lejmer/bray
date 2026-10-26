use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::{BoundExpressionId, MemoryOperationStatus};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, SeverityKind,
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

    pub(super) fn add_memory_diagnostic(
        &mut self,
        state: &StorageFlowState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        status: MemoryOperationStatus,
    ) {
        let Some(kind) = memory_diagnostic_kind(status) else {
            return;
        };

        let origins = self.memory_failure_origins(state, operation, status);
        let expression = operation.expression();

        if !self.reported_memory_diagnostics.insert((kind, expression)) {
            return;
        }

        let node = self
            .request
            .view()
            .expression(expression)
            .unwrap_or_else(|| {
                panic!("checked memory operation {expression:?} retains its bound expression")
            });

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
                let node = self.request.view().expression(origin).unwrap_or_else(|| {
                    panic!("memory failure origin {origin:?} retains its bound expression")
                });

                let source = self.request.source(node.origin().source_anchor());

                if source.span() != span {
                    diagnostic = diagnostic
                        .with_related_location(DiagnosticRelatedLocation::new(kind, source.span()));
                }
            }
        }

        self.diagnostics.add(diagnostic);
    }
}
