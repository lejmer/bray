mod expression;
mod model;
mod partition;
mod scope;
mod verify;

pub use model::{LoweringPlanFailure, LoweringPlanFailureCause, LoweringPlanKind};
pub use verify::VerifiedLoweringPlans;

pub(crate) use verify::{
    CleanupPlanLookupError, ScopeExitCleanupStatus, dependency_subject_exists,
};
