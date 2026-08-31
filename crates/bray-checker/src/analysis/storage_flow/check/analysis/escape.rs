use bray_bound_tree::{AnyBoundNodeId, BoundDependencySubject};
use bray_diagnostics::{
    Diagnostic, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote,
    DiagnosticNoteKind, DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, SeverityKind,
};

use super::core::StorageFlowCollector;
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::diagnostic::{bound_node_origin, diagnostic_id};
use crate::{CheckerInfrastructureError, CheckerRequestContext};

impl<C> StorageFlowCollector<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn report_escaping_storage_dependencies(
        &mut self,
        state: &StorageFlowState,
        block: bray_bound_tree::BoundBlockId,
        exit: AnyBoundNodeId,
    ) {
        if !self.publish || self.infrastructure_failure.is_some() {
            return;
        }

        for borrow in state.active_borrows.iter().copied() {
            let subject = BoundDependencySubject::BorrowCapability(borrow);

            if !self.liveness.is_owner_retained(subject)
                || !self.liveness.is_live_across_scope(block, subject)
            {
                continue;
            }

            let Some(capability) = self.storage.borrow_capability(borrow) else {
                self.record_infrastructure_failure(CheckerInfrastructureError::InvalidStorageFlow);

                return;
            };

            let Some(storage) = self.storage.root_identity(capability.access()) else {
                continue;
            };

            if self.owners.identity_scope(self.storage, storage) != Some(block)
                || !self.exit_leaves_scope(exit, block)
                || !self.reported_diagnostics.insert((
                    DiagnosticKind::CheckingEscapingStorageDependency,
                    capability.access(),
                ))
            {
                continue;
            }

            let Some(exit_origin) = bound_node_origin(self.request, exit) else {
                self.record_infrastructure_failure(CheckerInfrastructureError::InvalidStorageFlow);

                return;
            };

            let primary = match self.request.source(exit_origin.source_anchor()) {
                Ok(source) => source.span(),
                Err(error) => {
                    self.record_infrastructure_failure(error);

                    return;
                }
            };

            let dependency = match self.request.source(capability.source()) {
                Ok(source) => source.span(),
                Err(error) => {
                    self.record_infrastructure_failure(error);

                    return;
                }
            };

            self.diagnostics.add(
                Diagnostic::new(
                    diagnostic_id(self.diagnostics.len()),
                    DiagnosticKind::CheckingEscapingStorageDependency,
                    SeverityKind::Error,
                )
                .with_primary_span(primary)
                .with_label(DiagnosticLabel::primary(
                    DiagnosticLabelKind::EscapingStorageDependency,
                    primary,
                ))
                .with_related_location(DiagnosticRelatedLocation::new(
                    DiagnosticRelatedLocationKind::DependencyStorageOrigin,
                    dependency,
                ))
                .with_note(DiagnosticNote::new(
                    DiagnosticNoteKind::EscapingStorageDependencyResolution,
                )),
            );
        }
    }

    fn exit_leaves_scope(
        &mut self,
        exit: AnyBoundNodeId,
        block: bray_bound_tree::BoundBlockId,
    ) -> bool {
        let AnyBoundNodeId::Expression(exit) = exit else {
            return true;
        };

        let Some(bray_bound_tree::BoundExpression::ControlTransfer(transfer)) =
            self.request.view().expression(exit)
        else {
            return true;
        };

        let Some(target) = transfer.target() else {
            return true;
        };

        let Some(block) = self.request.view().block(block) else {
            self.record_infrastructure_failure(CheckerInfrastructureError::InvalidStorageFlow);

            return false;
        };

        target != block.origin().source_anchor().syntax()
    }
}
