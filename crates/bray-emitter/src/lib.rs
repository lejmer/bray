//! Artifact emission lifecycle and immutable output contracts for Bray compiler products.

#![forbid(unsafe_code)]

#[expect(
    dead_code,
    reason = "BRA-166 and BRA-167 will use private artifact construction and validation"
)]
mod artifact;
#[expect(
    dead_code,
    reason = "BRA-167 will construct emission outcomes after atomic publication"
)]
mod outcome;
mod plan;
mod request;
mod sink;

#[cfg(test)]
mod test_support;

pub use artifact::{
    ArtifactContribution, ArtifactId, ArtifactKind, ArtifactProducer, ArtifactRequirement,
    ArtifactRole, DependencyMetadataProducerId, EmittedArtifact, EmittedArtifactSet,
    LinkerProducerId,
};
pub use bray_symbols::{ProductIdentity, ProductKind};
pub use outcome::{EmissionFailure, EmissionOutcome, EmissionStatus};
pub use plan::{
    BackendEmissionPolicy, EmissionBackend, EmissionBackendBuildError, EmissionPlan,
    EmissionPlanner, EmissionPlanningError, PackageInterfacePolicy, PlannedArtifact,
    PlannedArtifactDestination,
};
pub use request::{
    EmissionRequest, EmissionRequestBuildError, RequestedArtifact, RequestedArtifactDestination,
};
pub use sink::{OutputSink, OutputSinkId, ReplacementPolicy};
