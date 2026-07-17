use bray_bound_tree::{
    BoundBlockId, BoundExpressionId, BoundPatternId, CheckedBlockResultRole,
    CheckedControlTransferTarget,
};
use bray_declarations::SyntaxAnchor;

use crate::analysis::build::{ControlFlowGraphBuilder, LoopContext};
use crate::analysis::id::AnalysisBlockId;
use crate::analysis::model::{AnalysisEdgeKind, AnalysisExitKind};

impl ControlFlowGraphBuilder<'_> {
    pub(super) fn build_while(
        &mut self,
        id: BoundExpressionId,
        condition: &[BoundExpressionId],
        body: BoundBlockId,
        else_body: Option<BoundBlockId>,
        target: SyntaxAnchor,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let header = self.push_block();
        let body_entry = self.push_block();
        let exhausted = self.push_block();
        let join = self.push_block();

        self.push_edge(current, header, AnalysisEdgeKind::Sequential, None);

        let condition = self.build_operands(condition, header)?;

        self.push_bound(condition, id.into());
        self.push_edge(condition, body_entry, AnalysisEdgeKind::LoopEntry, None);
        self.push_edge(
            condition,
            exhausted,
            AnalysisEdgeKind::ConditionalFalse,
            None,
        );

        self.loops.push(LoopContext {
            target,
            checked_target: CheckedControlTransferTarget::Loop(id),
            continue_target: Some(header),
            completion: join,
            scope_depth: self.scope_depth(),
        });

        if let Some(body_exit) =
            self.build_block_with_role(body, body_entry, CheckedBlockResultRole::LoopBody(id))?
        {
            self.push_edge(body_exit, header, AnalysisEdgeKind::LoopBack, None);
        }

        if let Some(context) = self.loops.last_mut() {
            context.continue_target = None;
        }

        match else_body {
            Some(else_body) => {
                if let Some(else_exit) = self.build_block_with_role(
                    else_body,
                    exhausted,
                    CheckedBlockResultRole::Expression(id),
                )? {
                    self.push_edge(else_exit, join, AnalysisEdgeKind::Sequential, None);
                }
            }
            None => self.push_edge(exhausted, join, AnalysisEdgeKind::Sequential, None),
        }

        self.loops.pop();

        Some(Some(join))
    }

    pub(super) fn build_unconditional_loop(
        &mut self,
        id: BoundExpressionId,
        body: BoundBlockId,
        target: SyntaxAnchor,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let header = self.push_block();
        let body_entry = self.push_block();
        let join = self.push_block();

        self.push_edge(current, header, AnalysisEdgeKind::Sequential, None);
        self.push_bound(header, id.into());
        self.push_edge(header, body_entry, AnalysisEdgeKind::LoopEntry, None);
        self.push_exit(header, AnalysisExitKind::Divergence);

        self.loops.push(LoopContext {
            target,
            checked_target: CheckedControlTransferTarget::Loop(id),
            continue_target: Some(header),
            completion: join,
            scope_depth: self.scope_depth(),
        });

        if let Some(body_exit) =
            self.build_block_with_role(body, body_entry, CheckedBlockResultRole::LoopBody(id))?
        {
            self.push_edge(body_exit, header, AnalysisEdgeKind::LoopBack, None);
        }

        self.loops.pop();

        Some(Some(join))
    }

    pub(in crate::analysis) fn build_iteration(
        &mut self,
        expression: BoundExpressionId,
        pattern: BoundPatternId,
        body: BoundBlockId,
        else_body: Option<BoundBlockId>,
        target: SyntaxAnchor,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let header = self.push_block();
        let item = self.push_block();
        let exhausted = self.push_block();
        let join = self.push_block();

        self.push_edge(current, header, AnalysisEdgeKind::Sequential, None);
        self.push_edge(header, item, AnalysisEdgeKind::LoopEntry, None);
        self.push_edge(header, exhausted, AnalysisEdgeKind::ConditionalFalse, None);

        self.loops.push(LoopContext {
            target,
            checked_target: CheckedControlTransferTarget::Iteration(expression),
            continue_target: Some(header),
            completion: join,
            scope_depth: self.scope_depth(),
        });

        let item = self
            .build_pattern(pattern, item)?
            .unwrap_or_else(|| self.push_block());

        let body_role = match self.view().expression(expression) {
            Some(bray_bound_tree::BoundExpression::Generator(_)) => {
                CheckedBlockResultRole::GeneratorBody(expression)
            }
            _ => CheckedBlockResultRole::IterationBody(expression),
        };

        if let Some(body_exit) = self.build_block_with_role(body, item, body_role)? {
            self.push_edge(body_exit, header, AnalysisEdgeKind::LoopBack, None);
        }

        if let Some(context) = self.loops.last_mut() {
            context.continue_target = None;
        }

        match else_body {
            Some(else_body) => {
                if let Some(else_exit) = self.build_block_with_role(
                    else_body,
                    exhausted,
                    CheckedBlockResultRole::Expression(expression),
                )? {
                    self.push_edge(else_exit, join, AnalysisEdgeKind::Sequential, None);
                }
            }
            None => self.push_edge(exhausted, join, AnalysisEdgeKind::Sequential, None),
        }

        self.loops.pop();

        Some(Some(join))
    }
}
