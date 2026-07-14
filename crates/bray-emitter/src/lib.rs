//! Artifact emission lifecycle and immutable output contracts for Bray compiler products.

#![forbid(unsafe_code)]

mod artifact;
mod outcome;
mod plan;
mod request;
mod sink;

#[cfg(test)]
mod test_support;

pub use artifact::{
    ArtifactContribution, ArtifactId, ArtifactKind, ArtifactProducer, ArtifactRequirement,
    ArtifactRole, DependencyMetadataProducerId, EmittedArtifact, EmittedArtifactSet,
    EmittedArtifactSetBuildError, LinkerProducerId,
};
pub use bray_symbols::{ProductIdentity, ProductKind};
pub use outcome::{EmissionFailure, EmissionOutcome, EmissionStatus};
pub use plan::{EmissionPlan, EmissionPlanBuildError, PlannedArtifact, PlannedArtifactDestination};
pub use request::{
    EmissionRequest, EmissionRequestBuildError, RequestedArtifact, RequestedArtifactDestination,
};
pub use sink::{OutputSink, OutputSinkId, ReplacementPolicy};
