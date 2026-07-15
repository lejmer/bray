//! Immutable native-link planning and result contracts.

#![forbid(unsafe_code)]

mod cancellation;
mod driver;
mod input;
mod outcome;
mod output;
mod plan;
mod policy;
mod target;

#[cfg(test)]
mod test_support;

pub use cancellation::LinkCancellation;
pub use driver::{LinkerDriverIdentity, LinkerDriverKind};
pub use input::{
    LinkInput, LinkInputBuildError, LinkInputId, LinkInputKind, LinkInputMode, LinkInputProvenance,
    LinkInputSource,
};
pub use outcome::{
    LinkFailure, LinkOutcome, LinkOutcomeBuildError, LinkStatus, LinkedArtifactSet,
    LinkedArtifactSetBuildError,
};
pub use output::{
    LinkedArtifact, LinkedArtifactKind, LinkedArtifactRequirement, LinkedProductKind,
    PlannedLinkedArtifact, StagingDestination, StagingDestinationBuildError, StagingDestinationId,
    StagingPathKey,
};
pub use plan::{
    LinkPlan, LinkPlanBuildError, LinkPlanBuilder, LinkSearchPath, LinkSearchPathBuildError,
    LinkSearchPathKind, LinkSymbolName,
};
pub use policy::{
    DeadStripPolicy, DebugLinkPolicy, LinkPolicy, LinkSubsystem, SectionGarbageCollectionPolicy,
};
pub use target::{LinkModel, LinkTarget, LinkTargetBuildError};
