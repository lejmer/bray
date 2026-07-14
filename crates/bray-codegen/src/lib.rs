//! Backend-independent code generation orchestration.

#![forbid(unsafe_code)]

mod artifact;
mod backend;
mod cancellation;
mod options;
mod outcome;
mod request;
mod target;
mod unit;

pub use artifact::{
    ArtifactContent, ArtifactContentBuildError, ArtifactContentSource, ArtifactDigest,
    ArtifactDigestAlgorithm, BackendArtifactContribution, BackendArtifactKind,
    BackendArtifactRequest, BackendArtifactRequestBuildError, BackendArtifactSet,
    BackendArtifactSetBuildError,
};
pub use backend::{BackendCapabilities, BackendIdentity, BackendTargetPlatform, CodegenBackend};
pub use cancellation::CodegenCancellation;
pub use options::{CodegenOptions, DebugInformationMode, OptimizationLevel, SizePreference};
pub use outcome::{CodegenFailure, CodegenOutcome, CodegenStatus};
pub use request::CodegenRequest;
pub use target::{
    CodeModel, CodegenTarget, Endianness, ObjectFormat, RelocationModel, TargetArchitecture,
    TargetIdentity, TargetMachineProperties,
};
pub use unit::{CodegenUnit, CodegenUnitBuildError, CodegenUnitKey};
