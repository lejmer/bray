mod deferred;
mod unwind;
pub use unwind::{UnwindCall, UnwindCandidate, check_unwind_candidate};
mod bounds;
mod check;
mod cleanup;
pub(crate) use cleanup::{Cleanup, Plan as CleanupPlan};
mod completion;
mod integer;
mod operation;
mod ownership;
mod postcondition;
mod predicate;
mod retirement;
mod state;
mod termination;
mod trusted;

pub use check::check_execution_candidate;
pub(in crate::analysis) use flow::merge_execution;
pub(crate) use flow::{ExecutionFlowDomain, prepare_execution_flow};
pub(in crate::analysis) use state::ExecutionState;
pub(in crate::analysis) use trusted::{OwnedReturnCondition, TrustedChecks};
mod flow;

pub(in crate::analysis) use cleanup::{CleanupMutation, Lifecycle, LifecycleInvocation, Observer};
