use bray_bound_tree::{AnyBoundNodeId, BoundDependencySubject};
use bray_diagnostics::DiagnosticKind;

use super::core::StorageFlowCollector;
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::diagnostic::{bound_node_origin, diagnostic_id, escaping_storage_dependency_diagnostic};
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
                || !self.liveness.is_live_across_scope(block, exit, subject)
            {
                continue;
            }

            let Some(capability) = self.storage.borrow_capability(borrow) else {
                self.record_infrastructure_failure(CheckerInfrastructureError::StorageFlow(
                    crate::CheckerStorageFlowFailure::MissingBorrowCapability { borrow },
                ));

                return;
            };

            let Some(storage) = self.storage.root_identity(capability.access()) else {
                continue;
            };

            if self.borrow_has_permanent_literal_storage(capability) {
                continue;
            }

            if capability.entry_binding().is_some() {
                continue;
            }

            match self.borrow_reaches_external_storage(capability.access()) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => {
                    self.record_infrastructure_failure(error);
                    return;
                }
            }

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
                self.record_infrastructure_failure(CheckerInfrastructureError::StorageFlow(
                    crate::CheckerStorageFlowFailure::MissingExitOrigin { exit: exit.into() },
                ));

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

            self.diagnostics.add(escaping_storage_dependency_diagnostic(
                diagnostic_id(self.diagnostics.len()),
                primary,
                dependency,
            ));
        }
    }

    pub(super) fn report_escaping_default_storage(
        &mut self,
        state: &StorageFlowState,
        expression: bray_bound_tree::BoundExpressionId,
    ) {
        if !self.publish || self.infrastructure_failure.is_some() {
            return;
        }

        for borrow in state.active_borrows.iter().copied() {
            if !self
                .liveness
                .is_owner_retained_by(expression, BoundDependencySubject::BorrowCapability(borrow))
            {
                continue;
            }

            let Some(capability) = self.storage.borrow_capability(borrow) else {
                self.record_infrastructure_failure(CheckerInfrastructureError::StorageFlow(
                    crate::CheckerStorageFlowFailure::MissingBorrowCapability { borrow },
                ));

                return;
            };

            if capability.entry_binding().is_some()
                || self.borrow_has_permanent_literal_storage(capability)
            {
                continue;
            }

            let Some(identity) = self
                .storage
                .root_identity(capability.access())
                .and_then(|root| self.storage.identity(root))
            else {
                continue;
            };

            if identity.is_borrowed_provider_input(self.request.unit().key().kind())
                || matches!(identity, bray_bound_tree::StorageIdentity::Static(_))
            {
                continue;
            }

            match self.borrow_reaches_external_storage(capability.access()) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => {
                    self.record_infrastructure_failure(error);
                    return;
                }
            }

            if !self.reported_diagnostics.insert((
                DiagnosticKind::CheckingEscapingStorageDependency,
                capability.access(),
            )) {
                continue;
            }

            let diagnostic = (|| {
                let origin = bound_node_origin(self.request, expression.into()).ok_or(
                    CheckerInfrastructureError::StorageFlow(
                        crate::CheckerStorageFlowFailure::MissingExitOrigin {
                            exit: expression.into(),
                        },
                    ),
                )?;

                let source = self.request.source(origin.source_anchor())?;

                let dependency = identity
                    .definition_node()
                    .and_then(|node| bound_node_origin(self.request, node))
                    .map(|origin| origin.source_anchor())
                    .unwrap_or(capability.source());

                let dependency = self.request.source(dependency)?;

                Ok::<_, CheckerInfrastructureError>(escaping_storage_dependency_diagnostic(
                    diagnostic_id(self.diagnostics.len()),
                    source.span(),
                    dependency.span(),
                ))
            })();

            match diagnostic {
                Ok(diagnostic) => self.diagnostics.add(diagnostic),
                Err(error) => {
                    self.record_infrastructure_failure(error);
                    return;
                }
            }
        }
    }

    fn borrow_has_permanent_literal_storage(
        &self,
        capability: bray_bound_tree::PlannedBorrowCapability,
    ) -> bool {
        capability.kind() == bray_symbols::BorrowKind::Shared
            && matches!(
                self.storage.root_identity(capability.access())
                    .and_then(|root| self.storage.identity(root)),
                Some(bray_bound_tree::StorageIdentity::Temporary(expression))
                    if matches!(self.request.view().expression(expression),
                        Some(bray_bound_tree::BoundExpression::Literal(literal))
                            if literal.kind() == bray_bound_tree::BoundLiteralKind::String)
            )
    }

    fn borrow_reaches_external_storage(
        &self,
        mut access: bray_bound_tree::StorageAccessId,
    ) -> Result<bool, CheckerInfrastructureError> {
        loop {
            if self.projected_storage_borrow_kind(access)?.is_some() {
                return Ok(true);
            }

            let record =
                self.storage
                    .access(access)
                    .ok_or(CheckerInfrastructureError::StorageFlow(
                        crate::CheckerStorageFlowFailure::MissingStorageAccess { access },
                    ))?;

            let Some(borrow) = record.root().borrow_capability() else {
                return Ok(false);
            };

            let capability = self.storage.borrow_capability(borrow).ok_or(
                CheckerInfrastructureError::StorageFlow(
                    crate::CheckerStorageFlowFailure::MissingBorrowCapability { borrow },
                ),
            )?;

            if capability.entry_binding().is_some() {
                return Ok(true);
            }

            access = capability.access();
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
            self.record_infrastructure_failure(CheckerInfrastructureError::StorageFlow(
                crate::CheckerStorageFlowFailure::MissingBlock { block },
            ));

            return false;
        };

        target != block.origin().source_anchor().syntax()
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticBag, DiagnosticId, DiagnosticKind};
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};
    use bray_testing::assert_goal_state_diagnostic_kind;

    use crate::diagnostic::escaping_storage_dependency_diagnostic;

    #[test]
    fn escaping_storage_dependencies_publish_the_exact_goal_state_diagnostic() {
        let source = SourceId::new(0);
        let primary = SourceSpan::new(source, TextRange::new(TextSize::new(8), TextSize::new(16)));

        let dependency =
            SourceSpan::new(source, TextRange::new(TextSize::new(2), TextSize::new(7)));

        let diagnostics = DiagnosticBag::single(escaping_storage_dependency_diagnostic(
            DiagnosticId::new(0),
            primary,
            dependency,
        ));

        assert_goal_state_diagnostic_kind(
            &diagnostics,
            DiagnosticKind::CheckingEscapingStorageDependency,
        );
    }
}
