use bray_bound_tree::{BoundAwaitExpression, BoundExpressionId};

use super::build::ControlFlowGraphBuilder;
use super::id::AnalysisBlockId;
use super::model::{AnalysisEdgeKind, AnalysisExitKind};

impl ControlFlowGraphBuilder<'_> {
    pub(super) fn build_await(
        &mut self,
        id: BoundExpressionId,
        expression: BoundAwaitExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let current = self.build_expression(expression.operand(), current)?;
        let current = current.unwrap_or_else(|| self.push_block());

        self.push_bound(current, id.into());

        let suspended = self.push_block();
        let resume = self.push_block();
        let cancellation = self.push_block();

        self.push_edge(current, suspended, AnalysisEdgeKind::AsyncSuspend, None);
        self.push_edge(suspended, resume, AnalysisEdgeKind::AsyncResume, None);
        self.push_edge(suspended, cancellation, AnalysisEdgeKind::AsyncCancel, None);
        self.push_exit(cancellation, AnalysisExitKind::Cancellation);

        Some(Some(resume))
    }
}
