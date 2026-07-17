//! Backend-independent code generation orchestration.

#![forbid(unsafe_code)]

mod artifact;
mod backend;
mod options;
mod outcome;
mod request;
mod runtime;
mod target;
mod unit;

#[cfg(test)]
mod test_support;

pub use artifact::{
    ArtifactContent, ArtifactContentBuildError, ArtifactContentSource, ArtifactDigest,
    ArtifactDigestAlgorithm, ArtifactSpool, ArtifactSpoolError, ArtifactSpoolOperation,
    ArtifactSpoolWriter, AssemblySyntaxKind, BackendArtifactContribution, BackendArtifactId,
    BackendArtifactKind, BackendArtifactRequest, BackendArtifactRequestBuildError,
    BackendArtifactRequestEntry, BackendArtifactRequirement, BackendArtifactSet,
    BackendArtifactSetBuildError, BackendSerializationOptions, DebugInformationOutputMode,
    LinkableArtifactKind, LinkableArtifactRequirement,
};
pub use backend::{BackendCapabilities, BackendIdentity, BackendTargetPlatform, CodeGenerator};
pub use options::{CodegenOptions, DebugInformationMode, OptimizationLevel, SizePreference};
pub use outcome::{CodegenFailure, CodegenOutcome, CodegenStatus};
pub use request::{CodegenRequest, CodegenRequestBuildError};
pub use runtime::{
    CodegenRuntimeMetadata, CodegenRuntimeMetadataBuildError, ProtectedAsyncFrameMetadata,
    ProtectedAsyncFrameOperationNames,
};
pub use target::{
    CallableAbiMapping, CodegenLinkage, CodegenTarget, CodegenTargetBuildError, TargetAbi,
    TargetAbiBuildError, TargetAddressSpace, TargetAddressSpaceKind, TargetCallingConvention,
    TargetCompatibility, TargetContract, TargetDataLayout, TargetDataLayoutBuildError,
    TargetMachineSelection, TargetScalarKind, TargetScalarLayout, TargetScalarLayoutBuildError,
    TargetSymbolConvention, TargetSymbolConventionBuildError,
};
pub use unit::{CodegenUnit, CodegenUnitBuildError, CodegenUnitKey};
