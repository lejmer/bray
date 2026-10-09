use bray_bound_tree::{AnyBoundNodeId, BoundDependencySubject};
use bray_diagnostics::DiagnosticKind;

use super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::diagnostic::{bound_node_origin, diagnostic_id, escaping_storage_dependency_diagnostic};

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
        if !self.publish || self.query_failure.is_some() {
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
                panic!(
                    "A storage borrow identity has no retained capability. in report_escaping_storage_dependencies, borrow: {:?}",
                    borrow
                );
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

            if self.borrow_reaches_external_storage(capability.access()) {
                continue;
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
                panic!(
                    "A control-flow exit has no retained source origin. in report_escaping_storage_dependencies, exit: {:?}",
                    exit
                );
            };

            let primary = self.request.source(exit_origin.source_anchor()).span();
            let dependency = self.request.source(capability.source()).span();

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
        if !self.publish || self.query_failure.is_some() {
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
                panic!(
                    "A storage borrow identity has no retained capability. in report_escaping_default_storage, borrow: {:?}",
                    borrow
                );
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

            if self.borrow_reaches_external_storage(capability.access()) {
                continue;
            }

            if !self.reported_diagnostics.insert((
                DiagnosticKind::CheckingEscapingStorageDependency,
                capability.access(),
            )) {
                continue;
            }

            let diagnostic = {
                let origin = bound_node_origin(self.request, expression.into()).unwrap_or_else(|| panic!("report_escaping_default_storage requires bound node source origin, expression: {expression:?}"));

                let source = self.request.source(origin.source_anchor());

                let dependency = identity
                    .definition_node()
                    .and_then(|node| bound_node_origin(self.request, node))
                    .map(|origin| origin.source_anchor())
                    .unwrap_or(capability.source());

                let dependency = self.request.source(dependency);

                escaping_storage_dependency_diagnostic(
                    diagnostic_id(self.diagnostics.len()),
                    source.span(),
                    dependency.span(),
                )
            };

            self.diagnostics.add(diagnostic);
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
    ) -> bool {
        loop {
            if self.projected_storage_borrow_kind(access).is_some() {
                return true;
            }

            let record =
                self.storage
                    .access(access).unwrap_or_else(|| panic!("borrow_reaches_external_storage requires planned storage access, access: {access:?}"));

            let Some(borrow) = record.root().borrow_capability() else {
                return matches!(
                    self.storage
                        .root_identity(access)
                        .and_then(|root| self.storage.identity(root)),
                    Some(bray_bound_tree::StorageIdentity::Static(_))
                );
            };

            let capability = self.storage.borrow_capability(borrow).unwrap_or_else(|| panic!("borrow_reaches_external_storage requires planned borrow capability, access: {access:?}, borrow: {borrow:?}"));

            if capability.entry_binding().is_some() {
                return true;
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
            panic!(
                "A control-flow transfer names a block absent from its source body. in exit_leaves_scope, block: {:?}",
                block
            );
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
