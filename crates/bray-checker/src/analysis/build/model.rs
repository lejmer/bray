use bray_bound_tree::{
    BoundBlockId, BoundCallableBodyId, BoundUnitView, CheckedControlFlowFactsBuilder,
    CheckedControlTransferTarget,
};
use bray_declarations::SyntaxAnchor;

use crate::{UnitCheckRequest, UnitCheckRoot};

use crate::analysis::assembly::ControlFlowGraphAssembler;
use crate::analysis::id::AnalysisBlockId;
use crate::analysis::model::{AnalysisExitKind, ControlFlowGraph};

pub(crate) enum ControlFlowGraphBuildOutcome {
    Complete(Box<ControlFlowGraphBuild>),
    Cancelled,
}

pub(crate) struct ControlFlowGraphBuild {
    pub(super) graph: ControlFlowGraph,
    pub(super) facts: CheckedControlFlowFactsBuilder,
}

impl ControlFlowGraphBuild {
    pub(crate) fn into_parts(self) -> (ControlFlowGraph, CheckedControlFlowFactsBuilder) {
        (self.graph, self.facts)
    }
}

impl std::ops::Deref for ControlFlowGraphBuild {
    type Target = ControlFlowGraph;

    fn deref(&self) -> &Self::Target {
        &self.graph
    }
}

pub(crate) fn build_control_flow_graph(
    request: UnitCheckRequest<'_>,
) -> ControlFlowGraphBuildOutcome {
    let mut builder = ControlFlowGraphBuilder::new(request);
    let entry = builder.push_block();

    let completion = match request.root() {
        UnitCheckRoot::CallableBody(root) => builder.build_callable_body(root, entry),
        UnitCheckRoot::Expression(root) => builder.build_expression(root, entry),
        UnitCheckRoot::ExpressionSequence(root) => builder.build_block(root, entry),
    };

    let Some(completion) = completion else {
        return ControlFlowGraphBuildOutcome::Cancelled;
    };

    if let Some(completion) = completion {
        builder.push_exit(completion, AnalysisExitKind::NormalFallthrough);
    }

    ControlFlowGraphBuildOutcome::Complete(Box::new(builder.finish(entry)))
}

pub(in crate::analysis) struct ControlFlowGraphBuilder<'view> {
    pub(super) request: UnitCheckRequest<'view>,
    pub(super) view: BoundUnitView<'view>,
    pub(super) storage: ControlFlowGraphAssembler,
    pub(in crate::analysis) facts: CheckedControlFlowFactsBuilder,
    pub(in crate::analysis) loops: Vec<LoopContext>,
    pub(in crate::analysis) catches: Vec<CatchContext>,
    pub(super) scopes: Vec<BoundBlockId>,
    pub(super) block_targets: Vec<BlockTargetContext>,
    pub(super) callable: Option<BoundCallableBodyId>,
}

#[derive(Clone, Copy)]
pub(in crate::analysis) struct LoopContext {
    pub(in crate::analysis) target: SyntaxAnchor,
    pub(in crate::analysis) checked_target: CheckedControlTransferTarget,
    pub(in crate::analysis) continue_target: Option<AnalysisBlockId>,
    pub(in crate::analysis) completion: AnalysisBlockId,
    pub(in crate::analysis) scope_depth: usize,
}

#[derive(Clone, Copy)]
pub(super) struct BlockTargetContext {
    pub(super) target: SyntaxAnchor,
    pub(super) block: BoundBlockId,
}

#[derive(Clone, Copy)]
pub(in crate::analysis) struct CatchContext {
    pub(in crate::analysis) target: AnalysisBlockId,
    pub(in crate::analysis) scope_depth: usize,
}
