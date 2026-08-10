//! Artifact emission lifecycle and immutable output contracts for Bray compiler products.

#![forbid(unsafe_code)]

mod artifact;
mod generation;
mod link;
mod outcome;
mod plan;
mod publication;
mod request;
mod sink;

#[cfg(test)]
mod test_support;

pub use artifact::{
    ArtifactContribution, ArtifactId, ArtifactKind, ArtifactProducer, ArtifactRequirement,
    ArtifactRole, BackendContributionMergeError, BackendContributionMergeErrorKind,
    BackendContributionSet, DependencyMetadataProducerId, EmittedArtifact, EmittedArtifactSet,
    LinkerProducerId,
};
pub use bray_runtime_interface::ExecutableHostContract;
pub use bray_symbols::{ProductIdentity, ProductKind};
pub use generation::{ProductGenerationIdentity, PublishedProductGeneration};
pub use link::{
    LinkOutputStaging, LinkOutputStagingBuildError, LinkPlanConstructionError, LinkStaging,
    LinkStagingError, ProductLinkFacts, StagedArtifact, StagedArtifactBuildError,
    construct_link_plan,
};
pub use outcome::{EmissionFailure, EmissionOutcome, EmissionStatus};
pub use plan::{
    BackendEmissionPolicy, EmissionBackend, EmissionBackendBuildError, EmissionPlan,
    EmissionPlanner, EmissionPlanningError, PlannedArtifact, PlannedArtifactDestination,
};
pub use publication::ArtifactPublisher;
pub use publication::{PublishedGenerationReadError, resolve_published_artifact};
pub use request::{
    EmissionRequest, EmissionRequestBuildError, RequestedArtifact, RequestedArtifactDestination,
};
pub use sink::{
    IndirectOutputSink, ManagedArtifactPath, OutputSink, OutputSinkId, OutputSinkResolver,
    OutputSinkTransaction, ReplacementPolicy,
};
