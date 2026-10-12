mod assembly;
mod build;
mod check;
mod cleanup;
mod condition;
mod control;
mod execution;
mod fixed_point;
pub(crate) mod guarantee;
mod id;
pub(crate) mod liveness;
mod model;
mod prepare;
mod propagation;
mod reachability;
mod refinement;
mod result;
mod storage_flow;
mod storage_index;

pub(crate) use check::check_control_flow;
pub(crate) use execution::exclude_non_unwinding_edges;
pub use execution::implementation_hook_may_propagate_synchronous_panic;
pub use guarantee::{UnwindCandidate, check_execution_candidate, check_unwind_candidate};
pub(crate) use liveness::analyze_storage_liveness_with_graph;
pub(crate) use model::{
    AnalysisOperationKind, AnalysisSuspensionKind, AnalysisTaskOperationKind, ControlFlowGraph,
};
pub use prepare::{PreparedBodyAnalysis, prepare_body_analysis};
pub(crate) use reachability::analyze_reachability_excluding;
pub(crate) use refinement::prepare_refinements;
pub(crate) use result::check_callable_result;
pub(crate) use storage_flow::check_storage_flow_with_graph;
pub use storage_flow::{closed_type_is_copyable, type_is_copyable, type_is_copyable_in_context};
mod storage_invalidation;

pub(crate) use cleanup::exclude_cleanup_free_edges;

#[cfg(test)]
pub(crate) use build::{ControlFlowGraphBuildOutcome, build_storage_control_flow_graph};
