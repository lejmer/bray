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

        self.report_escaping_return_references(state, Some(block), exit);

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
                || !self.reported_diagnostics.insert((
                    DiagnosticKind::CheckingEscapingStorageDependency,
                    capability.access(),
                ))
            {
                continue;
            }

            self.report_escaping_access(exit, capability.access());
        }

        // Returned contracts can retain storage directly without a syntactic returned loan.
        // Exact reference identity is not needed to reject a dependency on a dying local.
        for access in self.input.retained_storage.iter().copied() {
            if !self.liveness.is_live_across_scope(
                block,
                exit,
                BoundDependencySubject::StorageAccess(access),
            ) || self.borrow_reaches_external_storage(access)
                || self.has_permanent_literal_storage(access)
            {
                continue;
            }

            let Some(storage) = self.storage.root_identity(access) else {
                continue;
            };

            if self.owners.identity_scope(self.storage, storage) != Some(block)
                || !self
                    .reported_diagnostics
                    .insert((DiagnosticKind::CheckingEscapingStorageDependency, access))
            {
                continue;
            }

            self.report_escaping_access(exit, access);
        }
    }

    fn report_escaping_return_references(
        &mut self,
        state: &StorageFlowState,
        block: Option<bray_bound_tree::BoundBlockId>,
        exit: AnyBoundNodeId,
    ) {
        let Some(bray_bound_tree::StorageBinding::Identity(result)) = self
            .storage
            .binding(bray_bound_tree::StorageBindingTarget::Result)
        else {
            return;
        };

        let Some(ty) = self.result_type() else {
            return;
        };

        for (slot, origin) in state.memory.reference_targets(result) {
            let crate::analysis::storage_flow::memory::Origin::Storage(path) = origin else {
                continue;
            };

            if block.is_some_and(|block| {
                self.owners.identity_scope(self.storage, path.root) != Some(block)
            }) {
                continue;
            }

            let identity = self
                .storage
                .identity(path.root)
                .expect("reference target has planned storage");

            if identity.is_borrowed_provider_input(self.request.unit().key().kind()) {
                continue;
            }

            let access = path
                .access_id(self.storage)
                .or_else(|| bray_bound_tree::StoragePath::root(path.root).access_id(self.storage))
                .expect("returned reference has an evaluated source storage access");

            if self.borrow_reaches_external_storage(access)
                || self.has_permanent_literal_storage(access)
            {
                continue;
            }

            match self.reference_kind(slot, ty) {
                Ok(Some(bray_symbols::DependencyReferenceKind::Borrow)) => {}
                Ok(_) => continue,
                Err(error) => {
                    self.query_failure = Some(error);
                    return;
                }
            }

            if self
                .reported_diagnostics
                .insert((DiagnosticKind::CheckingEscapingStorageDependency, access))
            {
            self.report_escaping_access(exit, access);
            }
        }
    }

    fn report_escaping_access(
        &mut self,
        exit: AnyBoundNodeId,
        access: bray_bound_tree::StorageAccessId,
    ) {
        let exit_origin = bound_node_origin(self.request, exit)
            .expect("a control-flow exit retains its source origin");

        let source = self
            .storage
            .access(access)
            .expect("retained dependency has its planned access")
            .source();

        self.diagnostics.add(escaping_storage_dependency_diagnostic(
            diagnostic_id(self.diagnostics.len()),
            self.request.source(exit_origin.source_anchor()).span(),
            self.request.source(source).span(),
        ));
    }

    pub(super) fn report_escaping_default_storage(
        &mut self,
        state: &StorageFlowState,
        expression: bray_bound_tree::BoundExpressionId,
    ) {
        if !self.publish || self.query_failure.is_some() {
            return;
        }

        self.report_escaping_return_references(state, None, expression.into());

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

        for access in self.input.retained_storage.iter().copied() {
            let subject = BoundDependencySubject::StorageAccess(access);

            if !self.liveness.is_owner_retained_by(expression, subject)
                || self.borrow_reaches_external_storage(access)
                || self.has_permanent_literal_storage(access)
            {
                continue;
            }

            let identity = self
                .storage
                .root_identity(access)
                .and_then(|root| self.storage.identity(root))
                .expect("retained storage access has its planned identity");

            if identity.is_borrowed_provider_input(self.request.unit().key().kind()) {
                continue;
            }

            if self
                .reported_diagnostics
                .insert((DiagnosticKind::CheckingEscapingStorageDependency, access))
            {
                self.report_escaping_access(expression.into(), access);
            }
        }
    }

    fn borrow_has_permanent_literal_storage(
        &self,
        capability: bray_bound_tree::PlannedBorrowCapability,
    ) -> bool {
        capability.kind() == bray_symbols::BorrowKind::Shared
            && self.has_permanent_literal_storage(capability.access())
    }

    fn has_permanent_literal_storage(&self, access: bray_bound_tree::StorageAccessId) -> bool {
        matches!(
            self.storage.root_identity(access)
                .and_then(|root| self.storage.identity(root)),
            Some(bray_bound_tree::StorageIdentity::Temporary(expression))
                if matches!(self.request.view().expression(expression),
                    Some(bray_bound_tree::BoundExpression::Literal(literal))
                        if literal.kind() == bray_bound_tree::BoundLiteralKind::String)
        )
    }

    pub(super) fn borrow_reaches_external_storage(
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
