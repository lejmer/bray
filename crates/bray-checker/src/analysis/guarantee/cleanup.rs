mod actions;
mod candidate;
mod check;
mod inputs;
mod plan;
mod selection;
mod transition;

pub(super) use candidate::ReachedCleanup;
pub(super) use check::{ReachedScopeCleanup, check_cleanup, check_scope_cleanup};
pub(in crate::analysis) use plan::Lifecycle;
pub(crate) use plan::{Cleanup, Plan};
pub(in crate::analysis) use transition::{CleanupMutation, CleanupObservation, LifecycleInvocation, Observer, CleanupSelection};
