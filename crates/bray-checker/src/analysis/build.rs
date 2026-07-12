use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockId, BoundBlockItem, BoundControlTransferKind, BoundExpression,
    BoundExpressionId, BoundOperator, BoundPatternId, BoundUnitView,
};
use bray_declarations::SyntaxAnchor;

use crate::{UnitCheckRequest, UnitCheckRoot};

use super::assembly::ControlFlowGraphAssembler;
use super::id::AnalysisBlockId;
use super::model::{AnalysisEdgeKind, AnalysisExitKind, AnalysisRefinement, ControlFlowGraph};

pub(crate) enum ControlFlowGraphBuildOutcome {
    Complete(ControlFlowGraph),
    Cancelled,
}

pub(crate) fn build_control_flow_graph(
    request: UnitCheckRequest<'_>,
) -> ControlFlowGraphBuildOutcome {
    let mut builder = ControlFlowGraphBuilder::new(request);
    let entry = builder.push_block();

    let completion = match request.root() {
        UnitCheckRoot::CallableBody(root) => builder.build_callable_body(root, entry),
        UnitCheckRoot::Expression(root) => builder.build_expression(root, entry),
    };

    let Some(completion) = completion else {
        return ControlFlowGraphBuildOutcome::Cancelled;
    };

    if let Some(completion) = completion {
        builder.push_exit(completion, AnalysisExitKind::NormalFallthrough);
    }

    ControlFlowGraphBuildOutcome::Complete(builder.finish(entry))
}

pub(super) struct ControlFlowGraphBuilder<'view> {
    request: UnitCheckRequest<'view>,
    view: BoundUnitView<'view>,
    storage: ControlFlowGraphAssembler,
    pub(super) loops: Vec<LoopContext>,
    pub(super) catches: Vec<AnalysisBlockId>,
}

#[derive(Clone, Copy)]
pub(super) struct LoopContext {
    pub(super) target: SyntaxAnchor,
    pub(super) continue_target: Option<AnalysisBlockId>,
    pub(super) completion: AnalysisBlockId,
}

impl<'view> ControlFlowGraphBuilder<'view> {
    fn new(request: UnitCheckRequest<'view>) -> Self {
        Self {
            request,
            view: request.view(),
            storage: ControlFlowGraphAssembler::new(request.view().unit()),
            loops: Vec::new(),
            catches: Vec::new(),
        }
    }

    fn build_callable_body(
        &mut self,
        root: bray_bound_tree::BoundCallableBodyId,
        block: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        if self.cancelled() {
            return None;
        }

        let Some(body) = self.view.callable_body(root) else {
            self.push_recovery(block, root.into());
            self.push_exit(block, AnalysisExitKind::Recovery);

            return Some(None);
        };

        self.push_bound(block, root.into());

        match body.kind() {
            bray_bound_tree::BoundCallableBodyKind::Block(body) => self.build_block(body, block),
            bray_bound_tree::BoundCallableBodyKind::Error(error) => match error.body() {
                Some(body) => self.build_block(body, block),
                None => {
                    self.push_recovery(block, root.into());
                    self.push_exit(block, AnalysisExitKind::Recovery);

                    Some(None)
                }
            },
        }
    }

    pub(super) fn build_block(
        &mut self,
        id: BoundBlockId,
        mut current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        if self.cancelled() {
            return None;
        }

        let Some(block) = self.view.block(id) else {
            self.push_recovery(current, id.into());

            return Some(Some(current));
        };

        self.push_bound(current, id.into());

        for item in block.items() {
            let next = self.build_block_item(item, current)?;

            current = match next {
                Some(next) => next,
                None => self.push_block(),
            };
        }

        Some(Some(current))
    }

    fn build_block_item(
        &mut self,
        item: &BoundBlockItem,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        match item {
            BoundBlockItem::LocalBinding(binding) => {
                let current = self.build_expression(binding.initializer(), current)?;
                let current = current.unwrap_or_else(|| self.push_block());

                self.build_pattern(binding.pattern(), current)
            }
            BoundBlockItem::LocalConstant(constant) => {
                self.build_expression(constant.initializer(), current)
            }
            BoundBlockItem::Expression(expression) => self.build_expression(*expression, current),
        }
    }

    pub(super) fn build_expression(
        &mut self,
        id: BoundExpressionId,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        if self.cancelled() {
            return None;
        }

        let Some(expression) = self.view.expression(id) else {
            self.push_recovery(current, id.into());

            return Some(Some(current));
        };

        match expression {
            BoundExpression::Binary(expression)
                if matches!(
                    expression.operator(),
                    BoundOperator::LogicalAnd | BoundOperator::LogicalOr
                ) =>
            {
                self.build_short_circuit(id, expression.operands(), expression.operator(), current)
            }
            BoundExpression::Structured(expression) => {
                self.build_structured(id, expression, current)
            }
            BoundExpression::For(expression) => self.build_for(id, expression, current),
            BoundExpression::Match(expression) => self.build_match(id, expression, current),
            BoundExpression::Generator(expression) => {
                let current = self.build_expression(expression.source(), current)?;
                let current = current.unwrap_or_else(|| self.push_block());

                self.push_bound(current, id.into());

                self.build_iteration(
                    expression.pattern(),
                    expression.body(),
                    None,
                    expression.region(),
                    current,
                )
            }
            BoundExpression::ControlTransfer(expression) => {
                let current = self.build_optional_operand(expression.operand(), current)?;

                self.push_bound(current, id.into());
                self.push_control_transfer(current, expression.kind(), expression.target());

                Some(None)
            }
            _ => self.build_sequential_expression(id, expression, current),
        }
    }

    fn build_sequential_expression(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundExpression,
        mut current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        for child in expression.child_expressions() {
            current = self
                .build_expression(child, current)?
                .unwrap_or_else(|| self.push_block());
        }

        for pattern in expression.child_patterns() {
            current = self
                .build_pattern(pattern, current)?
                .unwrap_or_else(|| self.push_block());
        }

        for block in expression.child_blocks() {
            current = self
                .build_block(block, current)?
                .unwrap_or_else(|| self.push_block());
        }

        self.push_bound(current, id.into());

        Some(Some(current))
    }

    pub(super) fn build_pattern(
        &mut self,
        id: BoundPatternId,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        if self.cancelled() {
            return None;
        }

        let Some(pattern) = self.view.pattern(id) else {
            self.push_recovery(current, id.into());

            return Some(Some(current));
        };

        for child in pattern.children() {
            self.build_pattern(*child, current)?;
        }

        self.push_bound(current, id.into());

        Some(Some(current))
    }

    fn build_optional_operand(
        &mut self,
        operand: Option<BoundExpressionId>,
        current: AnalysisBlockId,
    ) -> Option<AnalysisBlockId> {
        let Some(operand) = operand else {
            return Some(current);
        };

        self.build_expression(operand, current)
            .map(|completion| completion.unwrap_or_else(|| self.push_block()))
    }

    fn push_control_transfer(
        &mut self,
        block: AnalysisBlockId,
        kind: BoundControlTransferKind,
        target: Option<SyntaxAnchor>,
    ) {
        let target_loop = match target {
            Some(target) => self
                .loops
                .iter()
                .rev()
                .find(|context| context.target == target),
            None => self.loops.last(),
        }
        .copied();

        match kind {
            BoundControlTransferKind::Yield => self.push_exit(block, AnalysisExitKind::Yield),
            BoundControlTransferKind::Return => self.push_exit(block, AnalysisExitKind::Return),
            BoundControlTransferKind::Break => match target_loop {
                Some(context) => {
                    self.push_edge(block, context.completion, AnalysisEdgeKind::LoopBreak, None);
                }
                None => self.push_exit(block, AnalysisExitKind::Recovery),
            },
            BoundControlTransferKind::Continue => match target_loop {
                Some(context) => match context.continue_target {
                    Some(header) => {
                        self.push_edge(block, header, AnalysisEdgeKind::LoopContinue, None);
                    }
                    None => self.push_exit(block, AnalysisExitKind::Recovery),
                },
                None => self.push_exit(block, AnalysisExitKind::Recovery),
            },
        }
    }

    pub(super) fn push_block(&mut self) -> AnalysisBlockId {
        self.storage.push_block()
    }

    pub(super) fn push_bound(&mut self, block: AnalysisBlockId, node: AnyBoundNodeId) {
        self.storage.push_bound(block, node);
    }

    pub(super) fn push_recovery(&mut self, block: AnalysisBlockId, node: AnyBoundNodeId) {
        self.storage.push_recovery(block, node);
    }

    pub(super) fn push_edge(
        &mut self,
        source: AnalysisBlockId,
        target: AnalysisBlockId,
        kind: AnalysisEdgeKind,
        refinement: Option<AnalysisRefinement>,
    ) {
        self.storage.push_edge(source, target, kind, refinement);
    }

    pub(super) fn push_exit(&mut self, block: AnalysisBlockId, kind: AnalysisExitKind) {
        if matches!(
            kind,
            AnalysisExitKind::Panic | AnalysisExitKind::Cancellation
        ) && let Some(catch) = self.catches.last().copied()
        {
            self.push_edge(block, catch, AnalysisEdgeKind::Catch, None);

            return;
        }

        self.storage.push_exit(block, kind);
    }

    fn cancelled(&self) -> bool {
        self.request.is_cancelled()
    }

    fn finish(self, entry: AnalysisBlockId) -> ControlFlowGraph {
        self.storage.finish(entry)
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        AnyBoundNodeId, BoundBinaryExpression, BoundBlock, BoundBlockItem, BoundCallableBody,
        BoundControlTransferExpression, BoundControlTransferKind, BoundErrorExpression,
        BoundExpression, BoundExpressionId, BoundForExpression, BoundMatchArm,
        BoundMatchExpression, BoundNodeOrigin, BoundOperator, BoundPattern, BoundPatternKind,
        BoundPatternMode, BoundStructuredExpression, BoundStructuredExpressionKind, BoundTree,
        BoundTreeBuilder, BoundUnitId, BoundUnitKey,
    };

    use super::{ControlFlowGraphBuildOutcome, build_control_flow_graph};
    use crate::analysis::model::ControlFlowGraph;
    use crate::analysis::model::{AnalysisEdgeKind, AnalysisExitKind, AnalysisOperationKind};
    use crate::test_support::{callable_key, error_type, recovered_tree};
    use crate::{UnitCheckRequest, UnitCheckRoot};

    #[test]
    fn recovered_nodes_produce_typed_recovery_operations_edges_and_exits() {
        let key = callable_key();
        let unit = BoundUnitId::new(6);
        let (tree, root) = recovered_tree(unit, &key);
        let view = tree.view(&key);

        let Ok(request) = UnitCheckRequest::new(view, UnitCheckRoot::CallableBody(root), &|| false)
        else {
            panic!("matching test roots must produce checker requests");
        };

        let ControlFlowGraphBuildOutcome::Complete(graph) = build_control_flow_graph(request)
        else {
            panic!("recovered graph construction must complete");
        };

        assert!(graph.is_well_formed());

        assert!(graph.operations().iter().any(|operation| matches!(
            operation.kind(),
            AnalysisOperationKind::Recovery(node) if node == root.into()
        )));

        assert!(
            graph
                .edges()
                .iter()
                .any(|edge| edge.kind() == AnalysisEdgeKind::Recovery)
        );

        assert!(
            graph
                .exits()
                .iter()
                .any(|exit| exit.kind() == AnalysisExitKind::Recovery)
        );
    }

    #[test]
    fn short_circuit_rhs_is_reached_only_through_its_required_branch() {
        let key = callable_key();
        let unit = BoundUnitId::new(7);
        let origin = BoundNodeOrigin::source(key.source());
        let mut builder = BoundTreeBuilder::new(unit);

        let left = push_error_expression(&mut builder, origin);
        let right = push_error_expression(&mut builder, origin);

        let binary = BoundExpression::Binary(BoundBinaryExpression::new(
            origin,
            BoundOperator::LogicalAnd,
            [left, right],
            Some(error_type()),
            false,
        ));

        let binary = push_expression(&mut builder, binary);
        let root = push_callable_root(&mut builder, origin, [binary]);
        let tree = builder.finish();
        let graph = graph(&tree, &key, root);

        let right_block = block_containing(&graph, right.into());
        let predecessor_kinds = right_block
            .predecessors()
            .iter()
            .filter_map(|edge| graph.edge(*edge).map(|edge| edge.kind()))
            .collect::<Vec<_>>();

        assert_eq!(predecessor_kinds, [AnalysisEdgeKind::ConditionalTrue]);
    }

    #[test]
    fn while_else_and_catch_paths_preserve_spec_evaluation_order() {
        let key = callable_key();
        let unit = BoundUnitId::new(8);
        let origin = BoundNodeOrigin::source(key.source());
        let mut builder = BoundTreeBuilder::new(unit);

        let condition = push_error_expression(&mut builder, origin);
        let body_value = push_error_expression(&mut builder, origin);
        let else_value = push_error_expression(&mut builder, origin);

        let body = push_block(&mut builder, origin, [body_value]);
        let else_body = push_block(&mut builder, origin, [else_value]);

        let while_expression = BoundExpression::Structured(BoundStructuredExpression::new(
            origin,
            BoundStructuredExpressionKind::While,
            [condition],
            [body, else_body],
            [],
            Some(error_type()),
            false,
        ));

        let while_expression = push_expression(&mut builder, while_expression);

        let panic_expression = BoundExpression::Structured(BoundStructuredExpression::new(
            origin,
            BoundStructuredExpressionKind::Panic,
            [],
            [],
            [],
            Some(error_type()),
            false,
        ));

        let panic_expression = push_expression(&mut builder, panic_expression);

        let catch_expression = BoundExpression::Structured(BoundStructuredExpression::new(
            origin,
            BoundStructuredExpressionKind::Catch,
            [panic_expression],
            [],
            [],
            Some(error_type()),
            false,
        ));

        let catch_expression = push_expression(&mut builder, catch_expression);
        let root = push_callable_root(&mut builder, origin, [while_expression, catch_expression]);

        let tree = builder.finish();
        let graph = graph(&tree, &key, root);

        let else_block = block_containing(&graph, else_value.into());

        assert!(else_block.predecessors().iter().any(|edge| {
            graph
                .edge(*edge)
                .is_some_and(|edge| edge.kind() == AnalysisEdgeKind::ConditionalFalse)
        }));

        assert!(
            !graph
                .exits()
                .iter()
                .any(|exit| exit.kind() == AnalysisExitKind::Panic)
        );

        assert!(
            graph
                .edges()
                .iter()
                .any(|edge| edge.kind() == AnalysisEdgeKind::Catch)
        );
    }

    #[test]
    fn for_exhaustion_and_match_guard_failure_use_distinct_paths() {
        let key = callable_key();
        let unit = BoundUnitId::new(9);
        let origin = BoundNodeOrigin::source(key.source());
        let mut builder = BoundTreeBuilder::new(unit);

        let source = push_error_expression(&mut builder, origin);
        let iteration_pattern = push_pattern(&mut builder, origin, BoundPatternMode::Declaration);

        let break_expression =
            BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
                origin,
                BoundControlTransferKind::Break,
                None,
                None,
                Some(error_type()),
                false,
            ));

        let break_expression = push_expression(&mut builder, break_expression);
        let for_body = push_block(&mut builder, origin, [break_expression]);
        let else_value = push_error_expression(&mut builder, origin);
        let for_else = push_block(&mut builder, origin, [else_value]);

        let for_expression = BoundExpression::For(BoundForExpression::new(
            origin,
            source,
            iteration_pattern,
            for_body,
            Some(for_else),
            Some(error_type()),
            false,
        ));

        let for_expression = push_expression(&mut builder, for_expression);

        let subject = push_error_expression(&mut builder, origin);
        let match_pattern = push_pattern(&mut builder, origin, BoundPatternMode::Match);
        let guard = push_error_expression(&mut builder, origin);
        let arm_value = push_error_expression(&mut builder, origin);
        let arm_body = push_block(&mut builder, origin, [arm_value]);

        let match_expression = BoundExpression::Match(BoundMatchExpression::new(
            origin,
            subject,
            [BoundMatchArm::new(match_pattern, Some(guard), arm_body)],
            Some(error_type()),
            false,
        ));

        let match_expression = push_expression(&mut builder, match_expression);
        let root = push_callable_root(&mut builder, origin, [for_expression, match_expression]);
        let tree = builder.finish();
        let graph = graph(&tree, &key, root);

        let else_block = block_containing(&graph, else_value.into());

        assert!(else_block.predecessors().iter().all(|edge| {
            graph
                .edge(*edge)
                .is_some_and(|edge| edge.kind() != AnalysisEdgeKind::LoopBreak)
        }));

        let arm_block = block_containing(&graph, arm_value.into());

        assert!(arm_block.predecessors().iter().any(|edge| {
            graph
                .edge(*edge)
                .is_some_and(|edge| edge.kind() == AnalysisEdgeKind::ConditionalTrue)
        }));

        let guard_block = block_containing(&graph, guard.into());

        assert!(guard_block.successors().iter().any(|edge| {
            graph
                .edge(*edge)
                .is_some_and(|edge| edge.kind() == AnalysisEdgeKind::ConditionalFalse)
        }));
    }

    fn graph(
        tree: &BoundTree,
        key: &BoundUnitKey,
        root: bray_bound_tree::BoundCallableBodyId,
    ) -> ControlFlowGraph {
        let view = tree.view(key);

        let Ok(request) = UnitCheckRequest::new(view, UnitCheckRoot::CallableBody(root), &|| false)
        else {
            panic!("matching test roots must produce checker requests");
        };

        let ControlFlowGraphBuildOutcome::Complete(graph) = build_control_flow_graph(request)
        else {
            panic!("valid graph construction must complete");
        };

        graph
    }

    fn push_error_expression(
        builder: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
    ) -> BoundExpressionId {
        push_expression(
            builder,
            BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
        )
    }

    fn push_expression(
        builder: &mut BoundTreeBuilder,
        expression: BoundExpression,
    ) -> BoundExpressionId {
        match builder.push_expression(expression) {
            Ok(expression) => expression,
            Err(error) => panic!("test expression must be valid: {error:?}"),
        }
    }

    fn push_pattern(
        builder: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
        mode: BoundPatternMode,
    ) -> bray_bound_tree::BoundPatternId {
        let pattern = BoundPattern::new(
            origin,
            error_type(),
            mode,
            BoundPatternKind::Discard,
            [],
            [],
        );

        match builder.push_pattern(pattern) {
            Ok(pattern) => pattern,
            Err(error) => panic!("test pattern must be valid: {error:?}"),
        }
    }

    fn push_block(
        builder: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
        expressions: impl IntoIterator<Item = BoundExpressionId>,
    ) -> bray_bound_tree::BoundBlockId {
        let block = BoundBlock::new(
            origin,
            expressions.into_iter().map(BoundBlockItem::Expression),
            false,
        );

        match builder.push_block(block) {
            Ok(block) => block,
            Err(error) => panic!("test block must be valid: {error:?}"),
        }
    }

    fn push_callable_root(
        builder: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
        expressions: impl IntoIterator<Item = BoundExpressionId>,
    ) -> bray_bound_tree::BoundCallableBodyId {
        let block = push_block(builder, origin, expressions);

        match builder.push_callable_body(BoundCallableBody::block(origin, block)) {
            Ok(root) => root,
            Err(error) => panic!("test callable root must be valid: {error:?}"),
        }
    }

    fn block_containing(
        graph: &ControlFlowGraph,
        node: AnyBoundNodeId,
    ) -> &crate::analysis::model::AnalysisBlock {
        let Some(block) = graph.blocks().iter().find(|block| {
            block.operations().iter().any(|operation| {
                graph
                    .operation(*operation)
                    .is_some_and(|operation| operation.kind() == AnalysisOperationKind::Bound(node))
            })
        }) else {
            panic!("test graph must retain the requested bound node");
        };

        block
    }
}
