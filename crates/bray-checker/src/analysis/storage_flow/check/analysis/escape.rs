use bray_bound_tree::{AnyBoundNodeId, BoundDependencySubject};
use bray_diagnostics::{DiagnosticId, DiagnosticKind};

use super::core::StorageFlowCollector;
use super::diagnostics::DiagnosticSubject;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::diagnostic::{bound_node_origin, escaping_storage_dependency_diagnostic};

impl<C> StorageFlowCollector<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn report_escaping_storage_dependencies(
        &mut self,
        state: &StorageFlowState,
        block: bray_bound_tree::StorageScope,
        exit: AnyBoundNodeId,
    ) {
        if !self.publish || self.query_failed {
            return;
        }

        self.report_escaping_return_references(state, Some(block), exit);

        let expires = |owner| match block {
            bray_bound_tree::StorageScope::AbandonedTransfer { value, .. } => owner == value,
            _ => self.execution.disposal.plan.storage_scope(owner) == Some(block),
        };

        let survives = |owner| match block {
            bray_bound_tree::StorageScope::AbandonedTransfer { .. } => !expires(owner),
            _ => !expires(owner)
                || bray_bound_tree::storage_identity_transfers_at_unit_exit(self.storage, owner),
        };

        let mut carried_borrows = self.returned_loans(state);
        let mut report_storage = std::collections::BTreeSet::new();

        for report in state.reachable_reports(survives) {
            carried_borrows.extend(report.subjects.iter().filter_map(|subject| {
                let BoundDependencySubject::BorrowCapability(borrow) = subject else { return None; };

                Some(*borrow)
            }));

            report_storage.extend(report.storage.iter().copied());
        }

        for &borrow in state.active_borrows.union(&carried_borrows) {
            let subject = BoundDependencySubject::BorrowCapability(borrow);

            if !carried_borrows.contains(&borrow)
                && (!self.liveness.is_owner_retained(subject)
                    || !self.liveness.is_live_across_scope(block, exit, subject))
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

            if self.storage.borrow_reaches_external_storage(capability.access()) {
                continue;
            }

            if !expires(storage) {
                continue;
            }

            self.report_escaping_access(exit, capability.access());
        }

        // Returned contracts can retain storage directly without a syntactic returned loan.
        // Exact reference identity is not needed to reject a dependency on a dying local.
        let live = self.execution.retained_storage.iter().copied().filter(|access| {
            self.liveness.is_live_across_scope(block, exit, BoundDependencySubject::StorageAccess(*access))
        });

        let expired = report_storage.into_iter().chain(live).filter(|access| {
            self.storage.root_identity(*access).is_some_and(expires)
                && !self.storage.borrow_reaches_external_storage(*access)
                && !self.has_permanent_literal_storage(*access)
        }).collect::<std::collections::BTreeSet<_>>();

        for access in expired {
            self.report_escaping_access(exit, access);
        }
    }

    fn returned_loans(&self, state: &StorageFlowState) -> std::collections::BTreeSet<bray_bound_tree::BorrowCapabilityId> {
        let Some(bray_bound_tree::StorageBinding::Identity(result)) = self.storage.binding(bray_bound_tree::StorageBindingTarget::Result) else {
            return Default::default();
        };

        let target = bray_bound_tree::StorageTarget::storage(bray_bound_tree::StoragePath::root(result));

        state.memory.value_loans(&target).collect()
    }

    fn report_escaping_return_references(
        &mut self,
        state: &StorageFlowState,
        block: Option<bray_bound_tree::StorageScope>,
        exit: AnyBoundNodeId,
    ) {
        let Some(bray_bound_tree::StorageBinding::Identity(result)) = self
            .storage
            .binding(bray_bound_tree::StorageBindingTarget::Result)
        else {
            return;
        };

        let Some(ty) = self.storage.identity_type(result) else {
            return;
        };

        for (slot, origin) in state.memory.reference_targets(result) {
            let crate::analysis::storage_flow::memory::Origin::Storage(path) = origin else {
                continue;
            };

            if block.is_some_and(|block| {
                self.execution.disposal.plan.storage_scope(path.root) != Some(block)
            }) {
                continue;
            }

            match self.retained_borrow_access(slot, origin, ty) {
                Ok(Some(access)) => self.report_escaping_access(exit, access),
                Ok(_) => continue,
                Err(error) => {
                    self.fail_query(error);
                    return;
                }
            }
        }
    }

    pub(super) fn retained_borrow_access(
        &self,
        slot: &bray_bound_tree::StoragePath,
        origin: &crate::analysis::storage_flow::memory::Origin,
        ty: bray_symbols::TypeId,
    ) -> crate::CheckerQueryResult<Option<bray_bound_tree::StorageAccessId>, C::UpstreamError> {
        let crate::analysis::storage_flow::memory::Origin::Storage(path) = origin else {
            return Ok(None);
        };

        let identity = self.storage.identity(path.root).expect("reference target has planned storage");

        if identity.borrows_input_storage(self.request.unit().key().kind()) {
            return Ok(None);
        }

        let access = path.access_id(self.storage)
            .or_else(|| self.storage.root_access(path.root))
            .expect("retained reference has an evaluated source storage access");

        if self.storage.borrow_reaches_external_storage(access) || self.has_permanent_literal_storage(access) {
            return Ok(None);
        }

        Ok((self.reference_kind(slot, ty)? == Some(bray_symbols::DependencyReferenceKind::Borrow)).then_some(access))
    }

    fn escape_span(&self, exit: AnyBoundNodeId) -> bray_source::SourceSpan {
        let origin = bound_node_origin(self.request, exit)
            .expect("a control-flow exit retains its source origin");

        let span = self.request.source(origin.source_anchor()).span();

        let AnyBoundNodeId::Expression(expression) = exit else {
            return span;
        };

        let Some(bray_bound_tree::BoundExpression::Call(call)) =
            self.request.view().expression(expression)
        else {
            return span;
        };

        let callee = crate::diagnostic::expression_span(self.request, call.callee());

        assert_eq!(
            span.source_id(),
            callee.source_id(),
            "a source invocation and its callee share a snapshot"
        );

        bray_source::SourceSpan::new(span.source_id(), span.range().cover(callee.range()))
    }

    fn report_escaping_access(
        &mut self,
        exit: AnyBoundNodeId,
        access: bray_bound_tree::StorageAccessId,
    ) {
        let exit_span = self.escape_span(exit);

        let source = self
            .storage
            .root_identity(access)
            .and_then(|identity| self.storage.identity(identity))
            .and_then(|identity| identity.definition_node())
            .and_then(|node| bound_node_origin(self.request, node))
            .map(|origin| origin.source_anchor())
            .unwrap_or_else(|| {
                self.storage
                    .access(access)
                    .expect("retained dependency has its planned access")
                    .source()
            });

        self.diagnostics.record(
            DiagnosticSubject::Storage(DiagnosticKind::CheckingEscapingStorageDependency, access),
            escaping_storage_dependency_diagnostic(DiagnosticId::new(0), exit_span, self.request.source(source).span()),
        );
    }

    pub(super) fn report_escaping_default_storage(
        &mut self,
        state: &StorageFlowState,
        expression: bray_bound_tree::BoundExpressionId,
    ) {
        if !self.publish || self.query_failed {
            return;
        }

        self.report_escaping_return_references(state, None, expression.into());

        let carried_borrows = self.returned_loans(state);

        for borrow in state.active_borrows.union(&carried_borrows).copied() {
            if !carried_borrows.contains(&borrow) && !self
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

            if identity.borrows_input_storage(self.request.unit().key().kind())
                || matches!(identity, bray_bound_tree::StorageIdentity::Static(_))
            {
                continue;
            }

            if self.storage.borrow_reaches_external_storage(capability.access()) {
                continue;
            }

            let diagnostic = {
                let source = self.escape_span(expression.into());

                let dependency = identity
                    .definition_node()
                    .and_then(|node| bound_node_origin(self.request, node))
                    .map(|origin| origin.source_anchor())
                    .unwrap_or(capability.source());

                let dependency = self.request.source(dependency);

                escaping_storage_dependency_diagnostic(
                    DiagnosticId::new(0),
                    source,
                    dependency.span(),
                )
            };

            self.diagnostics.record(
                DiagnosticSubject::Storage(DiagnosticKind::CheckingEscapingStorageDependency, capability.access()), diagnostic,
            );
        }

        for access in self.execution.retained_storage.iter().copied() {
            let subject = BoundDependencySubject::StorageAccess(access);

            if !self.liveness.is_owner_retained_by(expression, subject)
                || self.storage.borrow_reaches_external_storage(access)
                || self.has_permanent_literal_storage(access)
            {
                continue;
            }

            let identity = self
                .storage
                .root_identity(access)
                .and_then(|root| self.storage.identity(root))
                .expect("retained storage access has its planned identity");

            if identity.borrows_input_storage(self.request.unit().key().kind()) {
                continue;
            }

            self.report_escaping_access(expression.into(), access);
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
        self.storage
            .root_identity(access)
            .and_then(|root| self.storage.identity(root))
            .is_some_and(|identity| identity.has_permanent_literal_storage(self.request.view()))
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
