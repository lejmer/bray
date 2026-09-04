//! Artifact emission lifecycle and immutable output contracts for Bray compiler products.

#![forbid(unsafe_code)]

mod artifact;
mod build_identity;
mod generation;
mod link;
mod outcome;
mod plan;
mod publication;
mod request;
mod sink;
mod storage;

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
pub use build_identity::{
    BuildInputDigestError, ProductBuildIdentity, ProductBuildIdentityPart, build_input_path_digest,
    path_digest, toolchain_path_digest,
};
pub use generation::{ProductGenerationIdentity, PublishedProductGeneration};
pub use link::{
    LinkOutputStaging, LinkOutputStagingBuildError, LinkPlanConstructionError, LinkStaging,
    LinkStagingError, ProductLinkInputs, StagedArtifact, StagedArtifactBuildError,
    construct_link_plan,
};
pub use outcome::{EmissionFailure, EmissionOutcome, EmissionOutcomeBuildError, EmissionStatus};
pub use plan::{
    BackendEmissionPolicy, EmissionBackend, EmissionBackendBuildError, EmissionPlan,
    EmissionPlanner, EmissionPlanningError, PlannedArtifact, PlannedArtifactDestination,
};
pub use publication::{ArtifactPublisher, PublicationValidator};
pub use publication::{
    PublishedArtifact, PublishedGenerationReadError, PublishedProductReadGuard,
    RetainedProductGeneration, lock_published_product, resolve_published_artifact,
    retain_published_generation,
};
pub use request::{
    EmissionRequest, EmissionRequestBuildError, ManagedFilesystemDestination,
    ManagedOutputDirectory, RequestedArtifact, RequestedArtifactDestination,
};
pub use sink::{
    IndirectOutputSink, ManagedArtifactPath, OutputSink, OutputSinkId, OutputSinkResolver,
    OutputSinkTransaction, ReplacementPolicy,
};
pub use storage::{ManagedCache, ManagedOperation};
pub use storage::{
    StorageCategory, StorageEntryReport, StorageKind, StorageSelection, clean_storage,
    inspect_storage,
};
pub use storage::{StorageError, StorageErrorKind, StorageOperation, StoragePolicy};
