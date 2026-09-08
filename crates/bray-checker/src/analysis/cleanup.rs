use bray_bound_tree::{AnyBoundNodeId, BoundBlockId};

use crate::CheckerRequestContext;

use super::build::ControlFlowGraphBuilder;
use super::id::AnalysisBlockId;
use super::model::{AnalysisCleanupKind, AnalysisEdgeKind, AnalysisExitKind, AnalysisRefinement};

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

        self.storage
            .push_scope_exit(current, block, exit, AnalysisCleanupKind::Ordinary)
    }

    pub(super) fn push_exit(
        &mut self,
        block: AnalysisBlockId,
        kind: AnalysisExitKind,
        exit: AnyBoundNodeId,
    ) {
        let ordinary = matches!(
            kind,
            AnalysisExitKind::Return
                | AnalysisExitKind::ResultErrorPropagation
                | AnalysisExitKind::Yield
                | AnalysisExitKind::NormalFallthrough
        );

        if ordinary {
            self.push_cleanup_failures(block, 0, exit);
        }

        if kind == AnalysisExitKind::Panic
            && let Some(catch) = self.catches.last().copied()
        {
            let block = self.resolve_scopes(
                block,
                catch.scope_depth,
                exit,
                AnalysisCleanupKind::Abnormal,
            );

            self.push_edge(block, catch.target, AnalysisEdgeKind::Catch, None);

            return;
        }

        let block = match kind {
            AnalysisExitKind::Divergence => block,
            _ => self.resolve_scopes(
                block,
                0,
                exit,
                if ordinary {
                    AnalysisCleanupKind::Ordinary
                } else {
                    AnalysisCleanupKind::Abnormal
                },
            ),
        };

        self.storage.push_exit(block, kind, exit, None);
    }

    pub(super) fn push_cleanup_failures(
        &mut self,
        block: AnalysisBlockId,
        retained_depth: usize,
        exit: AnyBoundNodeId,
    ) {
        for index in (retained_depth..self.scopes.len()).rev() {
            if !self.cleanup_scopes.contains(&self.scopes[index]) {
                continue;
            }

            let refinement = Some(AnalysisRefinement::CleanupFailure {
                scope: self.scopes[index],
                exit,
            });

            let catch = self
                .catches
                .iter()
                .rev()
                .find(|catch| catch.scope_depth <= index)
                .copied();

            if let Some(catch) = catch {
                let failure = self.resolve_scopes(
                    block,
                    catch.scope_depth,
                    exit,
                    AnalysisCleanupKind::Abnormal,
                );

                self.push_edge(failure, catch.target, AnalysisEdgeKind::Catch, refinement);
            } else {
                let failure = self.resolve_scopes(block, 0, exit, AnalysisCleanupKind::Abnormal);

                self.storage
                    .push_exit(failure, AnalysisExitKind::Panic, exit, refinement);
            }

            let failure = self.resolve_scopes(block, 0, exit, AnalysisCleanupKind::Abnormal);

            self.storage
                .push_exit(failure, AnalysisExitKind::Cancellation, exit, refinement);
        }
    }

    pub(super) fn resolve_scopes(
        &mut self,
        mut current: AnalysisBlockId,
        retained_depth: usize,
        exit: AnyBoundNodeId,
        kind: AnalysisCleanupKind,
    ) -> AnalysisBlockId {
        for index in (retained_depth..self.scopes.len()).rev() {
            let scope = self.scopes[index];

            current = self.storage.push_scope_exit(current, scope, exit, kind);
        }

        current
    }
}
