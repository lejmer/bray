mod builder;
mod model;
mod policy;

pub use builder::{EmissionPlanner, EmissionPlanningError};
pub(crate) use model::EmissionPlanBuildError;
pub use model::{EmissionPlan, PlannedArtifact, PlannedArtifactDestination};
pub use policy::{
    BackendEmissionPolicy, EmissionBackend, EmissionBackendBuildError, PackageInterfacePolicy,
};
