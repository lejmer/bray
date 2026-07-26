use bray_bound_tree::{BoundAwaitExpression, BoundCallExpression, BoundExpressionId};

use crate::CheckerRequestContext;

use super::build::ControlFlowGraphBuilder;
use super::id::AnalysisBlockId;
use super::model::{AnalysisEdgeKind, AnalysisExitKind, AnalysisTaskOperationKind};

impl<C> ControlFlowGraphBuilder<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn build_await(
        &mut self,
        id: BoundExpressionId,
        expression: BoundAwaitExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let current = self.build_expression(expression.operand(), current)?;
        let current = current.unwrap_or_else(|| self.push_block());

        self.push_direct_await(current, id);

        let suspended = self.push_block();
        let resume = self.push_block();
        let cancellation = self.push_block();

        self.push_edge(current, suspended, AnalysisEdgeKind::AwaitSuspend, None);
        self.push_edge(suspended, resume, AnalysisEdgeKind::AwaitResume, None);

        self.push_edge(
            suspended,
            cancellation,
            AnalysisEdgeKind::RunCancellation,
            None,
        );

        self.push_exit(cancellation, AnalysisExitKind::Cancellation);

        Some(Some(resume))
    }

    pub(super) fn build_call(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundCallExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let operands = std::iter::once(expression.callee()).chain(
            expression
                .arguments()
                .iter()
                .map(|argument| argument.expression()),
        );

        let current = self.build_expressions(operands, current)?;

        let Some(kind) = self.task_operation_kind(expression) else {
            self.push_bound(current, id.into());

            return Some(Some(current));
        };

        self.push_task_operation(current, id, kind);

        Some(Some(current))
    }

    fn task_operation_kind(
        &self,
        expression: &BoundCallExpression,
    ) -> Option<AnalysisTaskOperationKind> {
        let target = expression.resolution().resolved()?.target().declaration()?;

        let hook = self
            .request()
            .available_compiler_known_symbols()
            .symbol_implementation(target.symbol())?;

        AnalysisTaskOperationKind::from_implementation_hook(hook)
    }
}
