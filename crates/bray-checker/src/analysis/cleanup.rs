use bray_bound_tree::{AnyBoundNodeId, BoundBlockId};

use crate::CheckerRequestContext;

use super::build::ControlFlowGraphBuilder;
use super::id::AnalysisBlockId;
use super::model::{AnalysisEdgeKind, AnalysisExitKind};

pub(crate) fn exclude_cleanup_free_edges(
    graph: &super::model::ControlFlowGraph,
    asynchronous: &bray_bound_tree::CheckedAsync,
    excluded: &mut std::collections::BTreeSet<super::id::AnalysisEdgeId>,
) -> bool {
    let cleanup_free = asynchronous
        .scope_exits()
        .iter()
        .filter(|plan| !plan.is_recovered() && !plan.has_cleanup())
        .map(|plan| (plan.scope(), plan.exit()))
        .collect::<std::collections::BTreeSet<_>>();

    let previous = excluded.len();

    excluded.extend(graph.edges().iter().filter_map(|edge| {
        let super::model::AnalysisRefinement::CleanupRequired { scope, exit } =
            edge.refinement()?
        else {
            return None;
        };

        cleanup_free.contains(&(scope, exit)).then_some(edge.id())
    }));

    excluded.len() != previous
}

impl<C> ControlFlowGraphBuilder<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    fn scoped_exit(
        &self,
        scope: BoundBlockId,
    ) -> Option<(
        bray_bound_tree::BoundExpressionId,
        bray_bound_tree::BoundCallResult,
    )> {
        let expression = *self.scoped_uses.get(&scope)?;

        let Some(bray_bound_tree::SemanticSelection::ScopedUse(scoped)) = self
            .selections()
            .and_then(|selections| selections.expression(expression))
        else {
            panic!("scoped cleanup inventory retains its selected exit");
        };

        Some((expression, scoped.exit_result()))
    }

    pub(super) fn push_scope_exit(
        &mut self,
        current: AnalysisBlockId,
        block: BoundBlockId,
        exit: AnyBoundNodeId,
    ) -> AnalysisBlockId {
        let retained_depth = self.scopes.len();
        let inserted = self.scopes.last() != Some(&block);

        if inserted {
            self.scopes.push(block);
        }

        self.push_cleanup_failures(current, self.scopes.len().saturating_sub(1), exit);
        self.scopes.truncate(retained_depth);

        self.storage
            .push_scope_exit(current, block, exit, true, self.scoped_exit(block))
    }

    pub(super) fn push_exit(
        &mut self,
        block: AnalysisBlockId,
        kind: AnalysisExitKind,
        exit: AnyBoundNodeId,
    ) {
        let preserves_exit_value = matches!(
            kind,
            AnalysisExitKind::Return
                | AnalysisExitKind::ResultErrorPropagation
                | AnalysisExitKind::Yield
                | AnalysisExitKind::NormalFallthrough
        );

        if preserves_exit_value {
            self.push_cleanup_failures(block, 0, exit);
        }

        if kind == AnalysisExitKind::Panic
            && let Some(catch) = self.catches.last().copied()
        {
            let block = self.resolve_scopes(block, catch.scope_depth, exit, false);

            self.push_edge(block, catch.target, AnalysisEdgeKind::Catch, None);

            return;
        }

        let block = match kind {
            AnalysisExitKind::Divergence => block,
            _ => self.resolve_scopes(block, 0, exit, preserves_exit_value),
        };

        self.storage.push_exit(block, kind);
    }

    pub(super) fn push_dependency_failures(
        &mut self,
        block: AnalysisBlockId,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) {
        let failure = self.push_block();

        self.push_edge(
            block,
            failure,
            AnalysisEdgeKind::Sequential,
            Some(super::model::AnalysisRefinement::InvocationFailure(
                invocation,
            )),
        );

        self.push_exit(failure, AnalysisExitKind::Panic, invocation.node());
        self.push_exit(failure, AnalysisExitKind::Cancellation, invocation.node());
    }

    pub(super) fn push_cleanup_failures(
        &mut self,
        block: AnalysisBlockId,
        retained_depth: usize,
        exit: AnyBoundNodeId,
    ) {
        let mut cancellation = None;

        for index in (retained_depth..self.scopes.len()).rev() {
            if !self.cleanup_scopes.contains(&self.scopes[index]) {
                continue;
            }

            let failure_entry = self.push_block();

            self.push_edge(
                block,
                failure_entry,
                AnalysisEdgeKind::Sequential,
                Some(super::model::AnalysisRefinement::CleanupRequired {
                    scope: self.scopes[index],
                    exit,
                }),
            );

            let catch = self
                .catches
                .iter()
                .rev()
                .find(|catch| catch.scope_depth <= index)
                .copied();

            if let Some(catch) = catch {
                let failure = self.resolve_scopes(failure_entry, catch.scope_depth, exit, false);

                self.push_edge(failure, catch.target, AnalysisEdgeKind::Catch, None);
            } else {
                let failure = self.resolve_scopes(failure_entry, 0, exit, false);

                self.storage.push_exit(failure, AnalysisExitKind::Panic);
            }

            let target = *cancellation.get_or_insert_with(|| self.push_block());

            self.push_edge(failure_entry, target, AnalysisEdgeKind::Sequential, None);
        }

        if let Some(cancellation) = cancellation {
            self.push_exit(cancellation, AnalysisExitKind::Cancellation, exit);
        }
    }

    pub(super) fn resolve_scopes(
        &mut self,
        mut current: AnalysisBlockId,
        retained_depth: usize,
        exit: AnyBoundNodeId,
        preserves_exit_value: bool,
    ) -> AnalysisBlockId {
        for index in (retained_depth..self.scopes.len()).rev() {
            let scope = self.scopes[index];

            current = self.storage.push_scope_exit(
                current,
                scope,
                exit,
                preserves_exit_value,
                self.scoped_exit(scope),
            );
        }

        current
    }
}
