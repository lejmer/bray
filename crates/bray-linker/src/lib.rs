//! Immutable native-link planning and result contracts.

#![forbid(unsafe_code)]

mod archive;
mod command;
mod driver;
mod execution;
mod external_tool;
mod input;
mod lld;
mod outcome;
mod output;
mod plan;
mod policy;
mod staging;
mod system;
mod target;

#[cfg(test)]
mod test_support;

pub use archive::{LlvmArchiveDriver, LlvmArchiveDriverBuildError};
pub use bray_runtime_interface::{BinarySymbolName, ExecutableHostContract};
pub use driver::{Linker, LinkerBuildError, LinkerDriver, LinkerDriverIdentity, LinkerDriverKind};
pub use external_tool::{
    ExternalToolFailure, ExternalToolHost, ExternalToolInvocation,
    ExternalToolInvocationBuildError, ExternalToolOutput, ExternalToolProcessBudget,
    ExternalToolResponseFile, ExternalToolResponseFileBuildError,
    ExternalToolResponseFileOperation, ExternalToolStream, NativeExternalToolHost,
};
pub use input::{
    LinkInput, LinkInputBuildError, LinkInputId, LinkInputKind, LinkInputMode, LinkInputProvenance,
    LinkInputSource, LinkInputSpec,
};
pub use lld::{EmbeddedLldHost, LldDriver, LldDriverBuildError, LldFlavor};
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
    LinkSearchPathKind,
};
pub use policy::{
    DeadStripPolicy, DebugLinkPolicy, LinkPolicy, LinkSubsystem, SectionGarbageCollectionPolicy,
};
pub use system::{
    SystemLinkerConfiguration, SystemLinkerConfigurationBuildError, SystemLinkerDriver,
    SystemLinkerDriverBuildError, SystemLinkerFamily,
};
pub use target::{LinkModel, LinkTarget, LinkTargetBuildError};
