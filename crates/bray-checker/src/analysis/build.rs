use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockId, BoundBlockItem, BoundControlTransferKind, BoundExpression,
    BoundExpressionId, BoundOperator, BoundPatternId, BoundUnitView, StoragePlan,
};
use bray_declarations::SyntaxAnchor;

use crate::{CheckerRequestContext, CheckerUnitRoot, CheckerUnitView};

use super::assembly::ControlFlowGraphAssembler;
use super::id::AnalysisBlockId;
use super::model::{
    AnalysisEdgeKind, AnalysisExitKind, AnalysisRefinement, AnalysisSuspensionKind,
    ControlFlowGraph,
};

pub(crate) enum ControlFlowGraphBuildOutcome {
    Complete(ControlFlowGraph),
    Cancelled,
}

pub(crate) fn build_control_flow_graph<C>(
    request: CheckerUnitView<'_, C>,
) -> ControlFlowGraphBuildOutcome
where
    C: CheckerRequestContext + ?Sized,
{
    build_control_flow_graph_with_storage(request, None, None)
}

pub(crate) fn build_storage_control_flow_graph<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    selections: &bray_bound_tree::CheckedSemanticSelections,
) -> ControlFlowGraphBuildOutcome
where
    C: CheckerRequestContext + ?Sized,
{
    build_control_flow_graph_with_storage(request, Some(storage), Some(selections))
}

fn build_control_flow_graph_with_storage<C>(
    request: CheckerUnitView<'_, C>,
    checked_storage: Option<&StoragePlan>,
    selections: Option<&bray_bound_tree::CheckedSemanticSelections>,
) -> ControlFlowGraphBuildOutcome
where
    C: CheckerRequestContext + ?Sized,
{
    let mut builder = ControlFlowGraphBuilder::new(request, checked_storage, selections);

    let entry = builder.push_block();

    let completion = match request.root() {
        CheckerUnitRoot::CallableBody(root) => builder.build_callable_body(root, entry),
        CheckerUnitRoot::Expression(root) => builder.build_expression(root, entry),
        CheckerUnitRoot::ExpressionSequence(root) => builder.build_block(root, entry),
    };

    let Some(completion) = completion else {
        return ControlFlowGraphBuildOutcome::Cancelled;
    };

    if let Some(completion) = completion {
        let exit = match request.root() {
            CheckerUnitRoot::CallableBody(root) => root.into(),
            CheckerUnitRoot::Expression(root) => root.into(),
            CheckerUnitRoot::ExpressionSequence(root) => root.into(),
        };

        builder.push_exit(completion, AnalysisExitKind::NormalFallthrough, exit);
    }

    ControlFlowGraphBuildOutcome::Complete(builder.finish(entry))
}

pub(super) struct ControlFlowGraphBuilder<'view, C>
where
    C: CheckerRequestContext + ?Sized,
{
    request: CheckerUnitView<'view, C>,
    view: BoundUnitView<'view>,
    storage: ControlFlowGraphAssembler,
    pub(super) loops: Vec<LoopContext>,
    pub(super) catches: Vec<CatchContext>,
    pub(super) yield_regions: Vec<SyntaxAnchor>,
    result_yields: Vec<ResultYieldContext>,
    scopes: Vec<BoundBlockId>,
    checked_storage: Option<&'view StoragePlan>,
    selections: Option<&'view bray_bound_tree::CheckedSemanticSelections>,
}

#[derive(Clone, Copy)]
pub(super) struct LoopContext {
    pub(super) target: SyntaxAnchor,
    pub(super) continue_target: Option<AnalysisBlockId>,
    pub(super) completion: AnalysisBlockId,
    pub(super) scope_depth: usize,
}

#[derive(Clone, Copy)]
pub(super) struct CatchContext {
    pub(super) target: AnalysisBlockId,
    pub(super) scope_depth: usize,
}

#[derive(Clone, Copy)]
struct ResultYieldContext {
    target: SyntaxAnchor,
    completion: AnalysisBlockId,
    scope_depth: usize,
}

impl<'view, C> ControlFlowGraphBuilder<'view, C>
where
    C: CheckerRequestContext + ?Sized,
{
    fn new(
        request: CheckerUnitView<'view, C>,
        checked_storage: Option<&'view StoragePlan>,
        selections: Option<&'view bray_bound_tree::CheckedSemanticSelections>,
    ) -> Self {
        Self {
            request,
            view: request.view(),
            storage: ControlFlowGraphAssembler::new(request.view().unit()),
            loops: Vec::new(),
            catches: Vec::new(),
            yield_regions: Vec::new(),
            result_yields: Vec::new(),
            scopes: Vec::new(),
            checked_storage,
            selections,
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
            self.push_exit(block, AnalysisExitKind::Recovery, root.into());

            return Some(None);
        };

        self.push_bound(block, root.into());

        match body.kind() {
            bray_bound_tree::BoundCallableBodyKind::Block(body) => self.build_block(body, block),
            bray_bound_tree::BoundCallableBodyKind::Error(error) => match error.body() {
                Some(body) => self.build_block(body, block),
                None => {
                    self.push_exit(block, AnalysisExitKind::Recovery, root.into());

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

        self.scopes.push(id);

        for item in block.items() {
            let next = self.build_block_item(item, current)?;

            current = match next {
                Some(next) => next,
                None => self.push_block(),
            };
        }

        let Some(scope) = self.scopes.pop() else {
            panic!("checker scope stack lost the active lexical block");
        };

        Some(Some(self.push_scope_exit(current, scope, id.into())))
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
            BoundExpression::Block(expression) => {
                self.build_block_expression(id, *expression, current)
            }
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
            BoundExpression::Call(expression) => self.build_call(id, expression, current),
            BoundExpression::Await(expression) => self.build_await(id, *expression, current),
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

                Some(self.push_control_transfer(
                    id,
                    current,
                    expression.kind(),
                    expression.target(),
                ))
            }
            _ => self.build_sequential_expression(id, expression, current),
        }
    }

    fn build_block_expression(
        &mut self,
        id: BoundExpressionId,
        expression: bray_bound_tree::BoundBlockExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let completion = self.push_block();

        self.result_yields.push(ResultYieldContext {
            target: expression.origin().source_anchor().syntax(),
            completion,
            scope_depth: self.scope_depth(),
        });

        let block_completion = self.build_block(expression.block(), current);

        self.result_yields.pop();

        if let Some(block_completion) = block_completion? {
            self.push_edge(
                block_completion,
                completion,
                AnalysisEdgeKind::Sequential,
                None,
            );
        }

        self.push_bound(completion, id.into());

        Some(Some(completion))
    }

    fn build_sequential_expression(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundExpression,
        mut current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        current = self.build_expressions(expression.child_expressions(), current)?;

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

    pub(super) fn build_expressions(
        &mut self,
        expressions: impl IntoIterator<Item = BoundExpressionId>,
        mut current: AnalysisBlockId,
    ) -> Option<AnalysisBlockId> {
        for expression in expressions {
            current = self
                .build_expression(expression, current)?
                .unwrap_or_else(|| self.push_block());
        }

        Some(current)
    }

    pub(super) fn build_operands(
        &mut self,
        operands: &[BoundExpressionId],
        current: AnalysisBlockId,
    ) -> Option<AnalysisBlockId> {
        self.build_expressions(operands.iter().copied(), current)
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
        id: BoundExpressionId,
        block: AnalysisBlockId,
        kind: BoundControlTransferKind,
        target: Option<SyntaxAnchor>,
    ) -> Option<AnalysisBlockId> {
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
            BoundControlTransferKind::Yield => {
                let target_result = target.and_then(|target| {
                    self.result_yields
                        .iter()
                        .rev()
                        .find(|context| context.target == target)
                        .copied()
                });

                if let Some(context) = target_result {
                    let block = self.resolve_scopes(block, context.scope_depth, id.into());

                    self.push_edge(block, context.completion, AnalysisEdgeKind::Yield, None);

                    return None;
                }

                if target.is_some_and(|target| self.yield_regions.contains(&target)) {
                    let continuation = self.push_block();

                    self.push_edge(block, continuation, AnalysisEdgeKind::Yield, None);

                    return Some(continuation);
                }

                self.push_exit(block, AnalysisExitKind::Yield, id.into());

                None
            }
            BoundControlTransferKind::Return => {
                self.push_exit(block, AnalysisExitKind::Return, id.into());

                None
            }
            BoundControlTransferKind::Break => match target_loop {
                Some(context) => {
                    let block = self.resolve_scopes(block, context.scope_depth, id.into());

                    self.push_edge(block, context.completion, AnalysisEdgeKind::LoopBreak, None);

                    None
                }
                None => {
                    self.push_exit(block, AnalysisExitKind::Recovery, id.into());

                    None
                }
            },
            BoundControlTransferKind::Continue => match target_loop {
                Some(context) => match context.continue_target {
                    Some(header) => {
                        let block = self.resolve_scopes(block, context.scope_depth, id.into());

                        self.push_edge(block, header, AnalysisEdgeKind::LoopContinue, None);

                        None
                    }
                    None => {
                        self.push_exit(block, AnalysisExitKind::Recovery, id.into());

                        None
                    }
                },
                None => {
                    self.push_exit(block, AnalysisExitKind::Recovery, id.into());

                    None
                }
            },
        }
    }

    pub(super) fn push_block(&mut self) -> AnalysisBlockId {
        self.storage.push_block()
    }

    pub(super) fn push_bound(&mut self, block: AnalysisBlockId, node: AnyBoundNodeId) {
        match self.view.node_is_recovered(node) {
            Some(false) => self.storage.push_bound(block, node),
            Some(true) | None => self.storage.push_recovery(block, node),
        }
    }

    pub(super) fn push_recovery(&mut self, block: AnalysisBlockId, node: AnyBoundNodeId) {
        self.storage.push_recovery(block, node);
    }

    pub(super) fn push_suspension(
        &mut self,
        block: AnalysisBlockId,
        expression: BoundExpressionId,
        kind: AnalysisSuspensionKind,
    ) {
        match self.view.node_is_recovered(expression.into()) {
            Some(false) => self.storage.push_suspension(block, expression, kind),
            Some(true) | None => self.storage.push_recovery(block, expression.into()),
        }
    }

    pub(super) fn push_task_operation(
        &mut self,
        block: AnalysisBlockId,
        expression: BoundExpressionId,
        kind: super::model::AnalysisTaskOperationKind,
    ) {
        match self.view.node_is_recovered(expression.into()) {
            Some(false) => self.storage.push_task_operation(block, expression, kind),
            Some(true) | None => self.storage.push_recovery(block, expression.into()),
        }
    }

    fn push_scope_exit(
        &mut self,
        current: AnalysisBlockId,
        block: BoundBlockId,
        exit: AnyBoundNodeId,
    ) -> AnalysisBlockId {
        self.storage.push_scope_exit(current, block, exit)
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

    pub(super) fn push_exit(
        &mut self,
        block: AnalysisBlockId,
        kind: AnalysisExitKind,
        exit: AnyBoundNodeId,
    ) {
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

    fn resolve_scopes(
        &mut self,
        mut current: AnalysisBlockId,
        retained_depth: usize,
        exit: AnyBoundNodeId,
    ) -> AnalysisBlockId {
        for index in (retained_depth..self.scopes.len()).rev() {
            let scope = self.scopes[index];

            current = self.push_scope_exit(current, scope, exit);
        }

        current
    }

    pub(super) const fn scope_depth(&self) -> usize {
        self.scopes.len()
    }

    pub(super) const fn request(&self) -> CheckerUnitView<'_, C> {
        self.request
    }

    pub(super) const fn view(&self) -> BoundUnitView<'_> {
        self.view
    }

    pub(super) const fn checked_storage(&self) -> Option<&StoragePlan> {
        self.checked_storage
    }

    pub(super) const fn selections(&self) -> Option<&bray_bound_tree::CheckedSemanticSelections> {
        self.selections
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
        AnyBoundNodeId, BoundAwaitExpression, BoundBinaryExpression, BoundBlockExpression,
        BoundBlockItem, BoundCallExpression, BoundCallResult, BoundCallableBody,
        BoundCallableTarget, BoundControlTransferExpression, BoundControlTransferKind,
        BoundErrorExpression, BoundExpression, BoundExpressionId, BoundForExpression,
        BoundFutureConstruction, BoundMatchArm, BoundMatchExpression, BoundNameExpression,
        BoundNodeOrigin, BoundOperator, BoundPattern, BoundPatternKind, BoundPatternMode,
        BoundReferenceTarget, BoundResolvedCall, BoundStructuredExpression,
        BoundStructuredExpressionKind, BoundTree, BoundTreeBuilder, BoundUnitId, BoundUnitKey,
    };
    use bray_compiler_known::{ImplementationHook, RepresentationRole};
    use bray_symbols::{
        CallableDefinitionId, CallableInstanceData, GenericArgument, GenericOwnerId,
        GenericParameterSymbolId, GenericSubstitutionData, NamedTypeSymbolId,
        TypeCallableMemberSymbolId, TypeData, TypeId, UnionSymbolId,
    };

    use super::{ControlFlowGraphBuildOutcome, build_control_flow_graph};
    use crate::CheckerUnitView;
    use crate::analysis::model::ControlFlowGraph;
    use crate::analysis::model::{
        AnalysisEdgeKind, AnalysisExitKind, AnalysisOperationKind, AnalysisScopeExitPhase,
        AnalysisSuspensionKind, AnalysisTaskOperationKind,
    };
    use crate::test_support::{
        TestCheckerContext, available_compiler_known_symbols, callable_entry, callable_key,
        callable_unit, error_type, push_block as push_bound_block, push_callable, push_expression,
        recovered_tree, semantic_values,
    };

    #[test]
    fn recovered_nodes_produce_typed_recovery_operations_edges_and_exits() {
        let key = callable_key();
        let unit = BoundUnitId::new(6);

        let (tree, root) = recovered_tree(unit, &key);

        let unit = callable_unit(&key, tree, root);
        let entry = callable_entry(&key);
        let context = TestCheckerContext::new(false);

        let Ok(request) = CheckerUnitView::new(&unit, &entry, &context) else {
            panic!("matching test roots must produce checker unit views");
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
    fn direct_await_has_suspend_resume_and_current_run_cancellation_paths() {
        let key = callable_key();
        let unit = BoundUnitId::new(10);
        let origin = BoundNodeOrigin::source(key.source());

        let mut builder = BoundTreeBuilder::new(unit);

        let operand = push_error_expression(&mut builder, origin);

        let await_expression = push_expression(
            &mut builder,
            BoundExpression::Await(BoundAwaitExpression::pending(origin, operand, false)),
        );

        let root = push_callable_root(&mut builder, origin, [await_expression]);

        let tree = builder.finish();
        let graph = graph(&tree, &key, root);

        for edge in [
            AnalysisEdgeKind::Suspension,
            AnalysisEdgeKind::Resume,
            AnalysisEdgeKind::RunCancellation,
        ] {
            assert!(
                graph
                    .edges()
                    .iter()
                    .any(|candidate| candidate.kind() == edge)
            );
        }

        assert!(
            graph
                .exits()
                .iter()
                .any(|exit| exit.kind() == AnalysisExitKind::Cancellation)
        );

        assert!(graph.operations().iter().any(|operation| matches!(
            operation.kind(),
            AnalysisOperationKind::Suspension {
                expression,
                kind: AnalysisSuspensionKind::Await,
            } if expression == await_expression
        )));
    }

    #[test]
    fn compiler_known_task_calls_retain_their_execution_roles() {
        let key = callable_key();
        let unit = BoundUnitId::new(11);
        let origin = BoundNodeOrigin::source(key.source());

        let mut builder = BoundTreeBuilder::new(unit);

        let callee = push_name_expression(&mut builder, origin, error_type());

        let start = push_task_call(
            &mut builder,
            origin,
            callee,
            ImplementationHook::FutureStart,
        );

        let join = push_task_call(&mut builder, origin, callee, ImplementationHook::TaskJoin);
        let cancel = push_task_call(&mut builder, origin, callee, ImplementationHook::TaskCancel);
        let root = push_callable_root(&mut builder, origin, [start, join, cancel]);

        let tree = builder.finish();
        let graph = graph(&tree, &key, root);

        let operations = graph
            .operations()
            .iter()
            .filter_map(|operation| match operation.kind() {
                AnalysisOperationKind::TaskOperation { expression, kind } => {
                    Some((expression, kind))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            operations,
            [
                (start, AnalysisTaskOperationKind::Start),
                (join, AnalysisTaskOperationKind::Join),
                (cancel, AnalysisTaskOperationKind::Cancel),
            ]
        );
    }

    #[test]
    fn run_result_try_forwards_panic_and_universally_bypasses_catch_for_cancellation() {
        let key = callable_key();
        let unit = BoundUnitId::new(12);
        let origin = BoundNodeOrigin::source(key.source());
        let run_result_type = representation_type(RepresentationRole::RunResult);

        let mut builder = BoundTreeBuilder::new(unit);

        let operand = push_name_expression(&mut builder, origin, run_result_type);

        let propagation = push_expression(
            &mut builder,
            BoundExpression::Structured(BoundStructuredExpression::new(
                origin,
                BoundStructuredExpressionKind::ResultPropagation,
                [operand],
                [],
                [],
                Some(error_type()),
                false,
            )),
        );

        let catching = push_expression(
            &mut builder,
            BoundExpression::Structured(BoundStructuredExpression::new(
                origin,
                BoundStructuredExpressionKind::Catch,
                [propagation],
                [],
                [],
                Some(error_type()),
                false,
            )),
        );

        let root = push_callable_root(&mut builder, origin, [catching]);

        let tree = builder.finish();
        let graph = graph(&tree, &key, root);

        for edge in [
            AnalysisEdgeKind::RunResultCompleted,
            AnalysisEdgeKind::RunResultPanicked,
            AnalysisEdgeKind::RunResultCancelled,
            AnalysisEdgeKind::Catch,
        ] {
            assert!(
                graph
                    .edges()
                    .iter()
                    .any(|candidate| candidate.kind() == edge)
            );
        }

        assert!(
            graph
                .exits()
                .iter()
                .any(|exit| exit.kind() == AnalysisExitKind::Cancellation)
        );

        assert!(
            !graph
                .exits()
                .iter()
                .any(|exit| exit.kind() == AnalysisExitKind::Panic)
        );
    }

    #[test]
    fn result_try_keeps_error_propagation_separate_from_run_forwarding() {
        let key = callable_key();
        let unit = BoundUnitId::new(14);
        let origin = BoundNodeOrigin::source(key.source());
        let result_type = representation_type(RepresentationRole::Result);

        let mut builder = BoundTreeBuilder::new(unit);

        let operand = push_name_expression(&mut builder, origin, result_type);

        let propagation = push_expression(
            &mut builder,
            BoundExpression::Structured(BoundStructuredExpression::new(
                origin,
                BoundStructuredExpressionKind::ResultPropagation,
                [operand],
                [],
                [],
                Some(error_type()),
                false,
            )),
        );

        let root = push_callable_root(&mut builder, origin, [propagation]);

        let tree = builder.finish();
        let graph = graph(&tree, &key, root);

        for edge in [
            AnalysisEdgeKind::ResultSuccess,
            AnalysisEdgeKind::ResultErrorPropagation,
        ] {
            assert!(
                graph
                    .edges()
                    .iter()
                    .any(|candidate| candidate.kind() == edge)
            );
        }

        assert!(!graph.edges().iter().any(|edge| matches!(
            edge.kind(),
            AnalysisEdgeKind::RunResultCompleted
                | AnalysisEdgeKind::RunResultPanicked
                | AnalysisEdgeKind::RunResultCancelled
        )));
    }

    #[test]
    fn lexical_exits_broadcast_task_cancellation_before_generic_lifecycle_resolution() {
        let key = callable_key();
        let unit = BoundUnitId::new(13);
        let origin = BoundNodeOrigin::source(key.source());

        let mut builder = BoundTreeBuilder::new(unit);

        let return_expression = push_expression(
            &mut builder,
            BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
                origin,
                BoundControlTransferKind::Return,
                None,
                None,
                Some(error_type()),
                false,
            )),
        );

        let inner = push_block(&mut builder, origin, [return_expression]);

        let inner_expression = push_expression(
            &mut builder,
            BoundExpression::Block(BoundBlockExpression::new(
                origin,
                inner,
                Some(error_type()),
                false,
            )),
        );

        let outer = push_block(&mut builder, origin, [inner_expression]);

        let root = match builder.push_callable_body(BoundCallableBody::block(origin, outer)) {
            Ok(root) => root,
            Err(error) => panic!("test callable root must be valid: {error:?}"),
        };

        let tree = builder.finish();
        let graph = graph(&tree, &key, root);

        let phases = reachable_scope_exit_phases(&graph);

        assert_eq!(
            phases,
            [
                (inner, AnalysisScopeExitPhase::TaskCancellationBroadcast),
                (inner, AnalysisScopeExitPhase::LifecycleResolution),
                (outer, AnalysisScopeExitPhase::TaskCancellationBroadcast),
                (outer, AnalysisScopeExitPhase::LifecycleResolution),
            ]
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
            bray_bound_tree::BoundIterationSource::new(
                source,
                bray_bound_tree::IterationSourceMode::Shared,
            ),
            iteration_pattern,
            for_body,
            Some(for_else),
            Some(error_type()),
            false,
        ));

        let for_expression = push_expression(&mut builder, for_expression);

        let subject = push_error_expression(&mut builder, origin);
        let match_pattern = push_pattern(&mut builder, origin, BoundPatternMode::MatchObserve);
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
        let unit = callable_unit(key, tree.clone(), root);
        let entry = callable_entry(key);
        let context = TestCheckerContext::new(false);

        let Ok(request) = CheckerUnitView::new(&unit, &entry, &context) else {
            panic!("matching test roots must produce checker unit views");
        };

        let ControlFlowGraphBuildOutcome::Complete(graph) = build_control_flow_graph(request)
        else {
            panic!("valid graph construction must complete");
        };

        graph
    }

    fn reachable_scope_exit_phases(
        graph: &ControlFlowGraph,
    ) -> Vec<(bray_bound_tree::BoundBlockId, AnalysisScopeExitPhase)> {
        let mut pending = vec![graph.entry()];
        let mut visited = vec![false; graph.blocks().len()];
        let mut phases = Vec::new();

        while let Some(block) = pending.pop() {
            let Some(index) = block.to_index() else {
                continue;
            };

            if visited.get(index).copied().unwrap_or(true) {
                continue;
            }

            visited[index] = true;

            let Some(block) = graph.block(block) else {
                continue;
            };

            for operation in block.operations() {
                let Some(operation) = graph.operation(*operation) else {
                    continue;
                };

                if let AnalysisOperationKind::ScopeExit { block, phase, .. } = operation.kind() {
                    phases.push((block, phase));
                }
            }

            for edge in block.successors().iter().rev() {
                if let Some(edge) = graph.edge(*edge) {
                    pending.push(edge.target());
                }
            }
        }

        phases
    }

    fn push_task_call(
        builder: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
        callee: BoundExpressionId,
        hook: ImplementationHook,
    ) -> BoundExpressionId {
        let definition = available_compiler_known_symbols()
            .implementation_symbols::<TypeCallableMemberSymbolId>(hook)
            .next()
            .unwrap_or_else(|| panic!("{hook:?} must be available to checker tests"));

        let callable = callable_instance(definition);

        let result = match hook {
            ImplementationHook::FutureStart => BoundCallResult::Immediate(error_type()),
            ImplementationHook::TaskJoin | ImplementationHook::TaskCancel => {
                BoundCallResult::LazyFuture(BoundFutureConstruction::new(
                    error_type(),
                    error_type(),
                ))
            }
            _ => panic!("test helper accepts only compiler-known task operations"),
        };

        push_expression(
            builder,
            BoundExpression::Call(BoundCallExpression::resolved(
                origin,
                callee,
                [],
                [],
                BoundResolvedCall::new(BoundCallableTarget::Declaration(callable), [], result),
            )),
        )
    }

    fn callable_instance(definition: TypeCallableMemberSymbolId) -> CallableInstanceData {
        let Some(definition) = CallableDefinitionId::try_new(definition.into()) else {
            panic!("compiler-known task methods must be callable definitions");
        };

        let substitution = empty_substitution(definition.symbol());

        CallableInstanceData::new(definition, substitution)
    }

    fn representation_type(role: RepresentationRole) -> TypeId {
        let Some(definition) =
            available_compiler_known_symbols().representation_symbol::<UnionSymbolId>(role)
        else {
            panic!("{role:?} must be available to checker tests");
        };

        let definition = NamedTypeSymbolId::from(definition);
        let substitution = empty_substitution(definition.into_any());

        match semantic_values().intern_type(TypeData::Named {
            definition,
            substitution,
        }) {
            Ok(ty) => ty,
            Err(error) => panic!("test representation type must be interned: {error:?}"),
        }
    }

    fn empty_substitution(
        symbol: bray_symbols::AnySymbolId,
    ) -> bray_symbols::GenericSubstitutionId {
        let Some(owner) = GenericOwnerId::try_new(symbol) else {
            panic!("test symbol must support a generic substitution");
        };

        let substitution = GenericSubstitutionData::try_new(
            owner,
            std::iter::empty::<GenericParameterSymbolId>(),
            std::iter::empty::<GenericArgument>(),
        );

        let substitution = match substitution {
            Ok(substitution) => substitution,
            Err(error) => panic!("empty test substitution must be valid: {error:?}"),
        };

        match semantic_values().intern_generic_substitution(substitution) {
            Ok(substitution) => substitution,
            Err(error) => panic!("empty test substitution must be interned: {error:?}"),
        }
    }

    fn push_name_expression(
        builder: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
        ty: TypeId,
    ) -> BoundExpressionId {
        let target = available_compiler_known_symbols()
            .representation_symbol::<UnionSymbolId>(RepresentationRole::RunResult)
            .map(Into::into)
            .unwrap_or_else(|| panic!("RunResult must be available to checker tests"));

        push_expression(
            builder,
            BoundExpression::Name(BoundNameExpression::new(
                origin,
                BoundReferenceTarget::Surface(target),
                Some(ty),
                false,
            )),
        )
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
        let items = expressions.into_iter().map(BoundBlockItem::Expression);

        push_bound_block(builder, origin, items)
    }

    fn push_callable_root(
        builder: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
        expressions: impl IntoIterator<Item = BoundExpressionId>,
    ) -> bray_bound_tree::BoundCallableBodyId {
        let block = push_block(builder, origin, expressions);

        push_callable(builder, origin, block)
    }

    fn block_containing(
        graph: &ControlFlowGraph,
        node: AnyBoundNodeId,
    ) -> &crate::analysis::model::AnalysisBlock {
        let Some(block) = graph.blocks().iter().find(|block| {
            block.operations().iter().any(|operation| {
                graph
                    .operation(*operation)
                    .is_some_and(|operation| operation.kind().node() == node)
            })
        }) else {
            panic!("test graph must retain the requested bound node");
        };

        block
    }
}
