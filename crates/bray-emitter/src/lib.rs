//! Artifact emission lifecycle and immutable output contracts for Bray compiler products.

#![forbid(unsafe_code)]

mod artifact;
mod outcome;
mod plan;
mod publication;
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
pub use publication::ArtifactPublisher;
pub use request::{
    EmissionRequest, EmissionRequestBuildError, RequestedArtifact, RequestedArtifactDestination,
};
pub use sink::{
    IndirectOutputSink, OutputSink, OutputSinkId, OutputSinkResolver, OutputSinkTransaction,
    ReplacementPolicy,
};
