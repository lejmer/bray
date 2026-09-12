use bray_bound_tree::{AnyBoundNodeId, BoundBlockId};

use crate::CheckerRequestContext;

use super::build::ControlFlowGraphBuilder;
use super::id::AnalysisBlockId;
use super::model::{AnalysisEdgeKind, AnalysisExitKind};

impl<C> ControlFlowGraphBuilder<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
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

        self.storage.push_scope_exit(current, block, exit)
    }

    pub(super) fn push_exit(
        &mut self,
        block: AnalysisBlockId,
        kind: AnalysisExitKind,
        exit: AnyBoundNodeId,
    ) {
        if matches!(
            kind,
            AnalysisExitKind::Return
                | AnalysisExitKind::ResultErrorPropagation
                | AnalysisExitKind::Yield
                | AnalysisExitKind::NormalFallthrough
        ) {
            self.push_cleanup_failures(block, 0, exit);
        }

        if kind == AnalysisExitKind::Panic
            && let Some(catch) = self.catches.last().copied()
        {
            let block = self.resolve_scopes(block, catch.scope_depth, exit);

            self.push_edge(block, catch.target, AnalysisEdgeKind::Catch, None);

            return;
        }

        let block = match kind {
            AnalysisExitKind::Divergence => block,
            _ => self.resolve_scopes(block, 0, exit),
        };

        self.storage.push_exit(block, kind);
    }

    pub(super) fn push_cleanup_failures(
        &mut self,
        block: AnalysisBlockId,
        retained_depth: usize,
        exit: AnyBoundNodeId,
    ) {
        let mut can_fail = false;

        for index in (retained_depth..self.scopes.len()).rev() {
            if !self.scope_cleanup_can_fail(self.scopes[index], exit) {
                continue;
            }

            can_fail = true;

            let catch = self
                .catches
                .iter()
                .rev()
                .find(|catch| catch.scope_depth <= index)
                .copied();

            if let Some(catch) = catch {
                let failure = self.resolve_scopes(block, catch.scope_depth, exit);
                self.push_edge(failure, catch.target, AnalysisEdgeKind::Catch, None);
            } else {
                let failure = self.resolve_scopes(block, 0, exit);
                self.storage.push_exit(failure, AnalysisExitKind::Panic);
            }
        }

        if can_fail {
            self.push_exit(block, AnalysisExitKind::Cancellation, exit);
        }
    }

    fn scope_cleanup_can_fail(&self, scope: BoundBlockId, exit: AnyBoundNodeId) -> bool {
        if !self.cleanup_scopes.contains(&scope) {
            return false;
        }

        let Some((_, _, asynchronous)) = self.completion_semantics else {
            return true;
        };

        let mut plans = asynchronous
            .scope_exits()
            .iter()
            .filter(|plan| plan.scope() == scope && plan.exit() == exit)
            .peekable();

        // Missing or recovered plans cannot rule out cleanup during recovery.
        plans.peek().is_none() || plans.any(|plan| plan.is_recovered() || plan.has_cleanup())
    }

    pub(super) fn resolve_scopes(
        &mut self,
        mut current: AnalysisBlockId,
        retained_depth: usize,
        exit: AnyBoundNodeId,
    ) -> AnalysisBlockId {
        for index in (retained_depth..self.scopes.len()).rev() {
            let scope = self.scopes[index];

            current = self.storage.push_scope_exit(current, scope, exit);
        }

        current
    }
}
