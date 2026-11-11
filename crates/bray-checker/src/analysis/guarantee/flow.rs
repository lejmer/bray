mod completion;
mod deferred;
mod inputs;
mod invalidation;
mod joins;
mod merge;
mod model;
mod numeric;
mod operation;
mod prepare;
mod refinement;
mod value;
mod witnesses;

pub(super) use joins::{merge_returned, returned_condition};
pub(in crate::analysis) use merge::merge as merge_execution;
pub(super) use model::ExecutionFlow;
pub(crate) use model::ExecutionFlowDomain;
pub(super) use prepare::analyze_execution_flow;
pub(crate) use prepare::prepare_execution_flow;

pub(super) use witnesses::owned_alternatives;
