use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockId, BoundBlockItem, BoundControlTransferKind, BoundExpression,
    BoundExpressionId, BoundPatternId, BoundStructuredExpressionKind, BoundUnitView,
};
use bray_declarations::SyntaxAnchor;

use crate::{UnitCheckRequest, UnitCheckRoot};

use super::assembly::TopologyStorage;
use super::id::AnalysisBlockId;
use super::model::{AnalysisEdgeKind, AnalysisExitKind, AnalysisRefinement, AnalysisTopology};

pub(crate) enum TopologyBuildOutcome {
    Complete(AnalysisTopology),
    Cancelled,
}

pub(crate) fn build_topology(request: UnitCheckRequest<'_>) -> TopologyBuildOutcome {
    let mut builder = TopologyBuilder::new(request);
    let entry = builder.push_block();

    if !request.root().accepts(request.view().kind()) {
        builder.push_recovery(entry, request.root().node());
        builder.push_exit(entry, AnalysisExitKind::Recovery);

        return TopologyBuildOutcome::Complete(builder.finish(entry));
    }

    let completion = match request.root() {
        UnitCheckRoot::CallableBody(root) => builder.build_callable_body(root, entry),
        UnitCheckRoot::Expression(root) => builder.build_expression(root, entry),
    };

    let Some(completion) = completion else {
        return TopologyBuildOutcome::Cancelled;
    };

    if let Some(completion) = completion {
        builder.push_exit(completion, AnalysisExitKind::NormalFallthrough);
    }

    TopologyBuildOutcome::Complete(builder.finish(entry))
}

struct TopologyBuilder<'view> {
    request: UnitCheckRequest<'view>,
    view: BoundUnitView<'view>,
    storage: TopologyStorage,
    loops: Vec<LoopContext>,
}

#[derive(Clone, Copy)]
struct LoopContext {
    target: SyntaxAnchor,
    header: AnalysisBlockId,
    completion: AnalysisBlockId,
}

impl<'view> TopologyBuilder<'view> {
    fn new(request: UnitCheckRequest<'view>) -> Self {
        Self {
            request,
            view: request.view(),
            storage: TopologyStorage::new(request.view().unit()),
            loops: Vec::new(),
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

    fn build_block(
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

    fn build_expression(
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
            BoundExpression::Structured(expression) => {
                self.build_structured(id, expression, current)
            }
            BoundExpression::For(expression) => self.build_for(id, expression, current),
            BoundExpression::Match(expression) => self.build_match(id, expression, current),
            BoundExpression::Generator(expression) => {
                let current = self.build_expression(expression.source(), current)?;
                let current = current.unwrap_or_else(|| self.push_block());

                self.push_bound(current, id.into());
                self.build_pattern(expression.pattern(), current)?;

                self.build_loop(expression.body(), expression.region(), current, true)
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

    fn build_structured(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundStructuredExpression,
        mut current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        for operand in expression.operands() {
            current = self
                .build_expression(*operand, current)?
                .unwrap_or_else(|| self.push_block());
        }

        self.push_bound(current, id.into());

        match expression.kind() {
            BoundStructuredExpressionKind::Conditional => {
                self.build_branches(expression.blocks(), current)
            }
            BoundStructuredExpressionKind::While | BoundStructuredExpressionKind::Loop => {
                let Some(body) = expression.blocks().first().copied() else {
                    self.push_recovery(current, id.into());

                    return Some(Some(current));
                };

                self.build_loop(
                    body,
                    expression.origin().source_anchor().syntax(),
                    current,
                    expression.kind() == BoundStructuredExpressionKind::While,
                )
            }
            BoundStructuredExpressionKind::ResultPropagation => {
                self.build_propagation(id, current, false)
            }
            BoundStructuredExpressionKind::NullablePropagation => {
                self.build_propagation(id, current, true)
            }
            BoundStructuredExpressionKind::Panic => {
                self.push_exit(current, AnalysisExitKind::Panic);

                Some(None)
            }
            BoundStructuredExpressionKind::Catch => {
                let join = self.push_block();

                self.push_edge(current, join, AnalysisEdgeKind::Sequential, None);

                for block in expression.blocks() {
                    let catch_entry = self.push_block();

                    self.push_edge(current, catch_entry, AnalysisEdgeKind::Catch, None);

                    let completion = self
                        .build_block(*block, catch_entry)?
                        .unwrap_or_else(|| self.push_block());

                    self.push_edge(completion, join, AnalysisEdgeKind::Sequential, None);
                }

                Some(Some(join))
            }
            BoundStructuredExpressionKind::Await => {
                let suspended = self.push_block();
                let resume = self.push_block();
                let cancellation = self.push_block();

                self.push_edge(current, suspended, AnalysisEdgeKind::AsyncSuspend, None);
                self.push_edge(suspended, resume, AnalysisEdgeKind::AsyncResume, None);
                self.push_edge(suspended, cancellation, AnalysisEdgeKind::AsyncCancel, None);
                self.push_exit(cancellation, AnalysisExitKind::Cancellation);

                Some(Some(resume))
            }
            BoundStructuredExpressionKind::AsyncBlock => {
                for block in expression.blocks() {
                    current = self
                        .build_block(*block, current)?
                        .unwrap_or_else(|| self.push_block());
                }

                let completion = self.push_block();

                self.push_edge(current, completion, AnalysisEdgeKind::TaskCompletion, None);

                Some(Some(completion))
            }
            _ => {
                for block in expression.blocks() {
                    current = self
                        .build_block(*block, current)?
                        .unwrap_or_else(|| self.push_block());
                }

                Some(Some(current))
            }
        }
    }

    fn build_for(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundForExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let current = self.build_expression(expression.source(), current)?;
        let current = current.unwrap_or_else(|| self.push_block());

        self.push_bound(current, id.into());
        self.build_pattern(expression.pattern(), current)?;

        let completion = self.build_loop(
            expression.body(),
            expression.origin().source_anchor().syntax(),
            current,
            true,
        )?;
        let Some(else_body) = expression.else_body() else {
            return Some(completion);
        };

        let completion = completion.unwrap_or_else(|| self.push_block());

        self.build_block(else_body, completion)
    }

    fn build_match(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundMatchExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let current = self.build_expression(expression.subject(), current)?;
        let current = current.unwrap_or_else(|| self.push_block());

        self.push_bound(current, id.into());

        let join = self.push_block();

        for arm in expression.arms() {
            let arm_entry = self.push_block();
            let refinement = AnalysisRefinement::PatternSuccess(arm.pattern());

            self.push_edge(
                current,
                arm_entry,
                AnalysisEdgeKind::MatchArm,
                Some(refinement),
            );

            let arm_entry = self
                .build_pattern(arm.pattern(), arm_entry)?
                .unwrap_or_else(|| self.push_block());

            let arm_entry = self.build_optional_operand(arm.guard(), arm_entry)?;

            if let Some(completion) = self.build_block(arm.body(), arm_entry)? {
                self.push_edge(completion, join, AnalysisEdgeKind::Sequential, None);
            }
        }

        self.push_edge(current, join, AnalysisEdgeKind::MatchNoMatch, None);

        Some(Some(join))
    }

    fn build_branches(
        &mut self,
        branches: &[BoundBlockId],
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let join = self.push_block();

        for (index, branch) in branches.iter().copied().enumerate() {
            let entry = self.push_block();
            let kind = if index == 0 {
                AnalysisEdgeKind::ConditionalTrue
            } else {
                AnalysisEdgeKind::ConditionalFalse
            };

            self.push_edge(current, entry, kind, None);

            if let Some(completion) = self.build_block(branch, entry)? {
                self.push_edge(completion, join, AnalysisEdgeKind::Sequential, None);
            }
        }

        if branches.len() < 2 {
            self.push_edge(current, join, AnalysisEdgeKind::ConditionalFalse, None);
        }

        Some(Some(join))
    }

    fn build_loop(
        &mut self,
        body: BoundBlockId,
        target: SyntaxAnchor,
        current: AnalysisBlockId,
        can_complete: bool,
    ) -> Option<Option<AnalysisBlockId>> {
        let body_entry = self.push_block();
        let completion = self.push_block();

        self.push_edge(current, body_entry, AnalysisEdgeKind::LoopEntry, None);

        if can_complete {
            self.push_edge(
                current,
                completion,
                AnalysisEdgeKind::ConditionalFalse,
                None,
            );
        } else {
            self.push_exit(current, AnalysisExitKind::Divergence);
        }

        self.loops.push(LoopContext {
            target,
            header: current,
            completion,
        });

        if let Some(body_exit) = self.build_block(body, body_entry)? {
            self.push_edge(body_exit, current, AnalysisEdgeKind::LoopBack, None);
        }

        self.loops.pop();

        Some(Some(completion))
    }

    fn build_propagation(
        &mut self,
        expression: BoundExpressionId,
        current: AnalysisBlockId,
        nullable: bool,
    ) -> Option<Option<AnalysisBlockId>> {
        let success = self.push_block();
        let failure = self.push_block();

        let (success_kind, failure_kind, success_refinement, failure_refinement) = if nullable {
            (
                AnalysisEdgeKind::NullablePresent,
                AnalysisEdgeKind::NullableAbsent,
                Some(AnalysisRefinement::NullablePresence {
                    expression,
                    is_present: true,
                }),
                Some(AnalysisRefinement::NullablePresence {
                    expression,
                    is_present: false,
                }),
            )
        } else {
            (
                AnalysisEdgeKind::ResultSuccess,
                AnalysisEdgeKind::ResultPropagation,
                None,
                None,
            )
        };

        self.push_edge(current, success, success_kind, success_refinement);
        self.push_edge(current, failure, failure_kind, failure_refinement);
        self.push_exit(failure, AnalysisExitKind::Propagation);

        Some(Some(success))
    }

    fn build_pattern(
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
                Some(context) => {
                    self.push_edge(block, context.header, AnalysisEdgeKind::LoopContinue, None);
                }
                None => self.push_exit(block, AnalysisExitKind::Recovery),
            },
        }
    }

    fn push_block(&mut self) -> AnalysisBlockId {
        self.storage.push_block()
    }

    fn push_bound(&mut self, block: AnalysisBlockId, node: AnyBoundNodeId) {
        self.storage.push_bound(block, node);
    }

    fn push_recovery(&mut self, block: AnalysisBlockId, node: AnyBoundNodeId) {
        self.storage.push_recovery(block, node);
    }

    fn push_edge(
        &mut self,
        source: AnalysisBlockId,
        target: AnalysisBlockId,
        kind: AnalysisEdgeKind,
        refinement: Option<AnalysisRefinement>,
    ) {
        self.storage.push_edge(source, target, kind, refinement);
    }

    fn push_exit(&mut self, block: AnalysisBlockId, kind: AnalysisExitKind) {
        self.storage.push_exit(block, kind);
    }

    fn cancelled(&self) -> bool {
        self.request.is_cancelled()
    }

    fn finish(self, entry: AnalysisBlockId) -> AnalysisTopology {
        self.storage.finish(entry)
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundUnitId;

    use super::{TopologyBuildOutcome, build_topology};
    use crate::analysis::model::{AnalysisEdgeKind, AnalysisExitKind, AnalysisOperationKind};
    use crate::test_support::{callable_key, recovered_tree};
    use crate::{UnitCheckRequest, UnitCheckRoot};

    #[test]
    fn recovered_nodes_produce_typed_recovery_operations_edges_and_exits() {
        let key = callable_key();
        let unit = BoundUnitId::new(6);
        let (tree, root) = recovered_tree(unit, &key);
        let view = tree.view(&key);

        let request = UnitCheckRequest::new(view, UnitCheckRoot::CallableBody(root), &|| false);

        let TopologyBuildOutcome::Complete(topology) = build_topology(request) else {
            panic!("recovered topology construction must complete");
        };

        assert!(topology.is_well_formed());

        assert!(topology.operations().iter().any(|operation| matches!(
            operation.kind(),
            AnalysisOperationKind::Recovery(node) if node == root.into()
        )));

        assert!(
            topology
                .edges()
                .iter()
                .any(|edge| edge.kind() == AnalysisEdgeKind::Recovery)
        );

        assert!(
            topology
                .exits()
                .iter()
                .any(|exit| exit.kind() == AnalysisExitKind::Recovery)
        );
    }
}
