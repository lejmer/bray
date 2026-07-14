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

#[cfg(test)]
mod test_support;

pub use artifact::{
    ArtifactContent, ArtifactContentBuildError, ArtifactContentSource, ArtifactDigest,
    ArtifactDigestAlgorithm, ArtifactSpool, ArtifactSpoolError, ArtifactSpoolOperation,
    ArtifactSpoolWriter, AssemblySyntaxKind, BackendArtifactContribution, BackendArtifactId,
    BackendArtifactKind, BackendArtifactRequest, BackendArtifactRequestBuildError,
    BackendArtifactRequestEntry, BackendArtifactRequirement, BackendArtifactSet,
    BackendArtifactSetBuildError, BackendSerializationOptions, DebugInformationOutputMode,
    LinkableArtifactRequirement,
};
pub use backend::{BackendCapabilities, BackendIdentity, BackendTargetPlatform, CodeGenerator};
pub use cancellation::CodegenCancellation;
pub use options::{CodegenOptions, DebugInformationMode, OptimizationLevel, SizePreference};
pub use outcome::{CodegenFailure, CodegenOutcome, CodegenStatus};
pub use request::{CodegenRequest, CodegenRequestBuildError};
pub use target::{
    CallableAbiMapping, CodeModel, CodegenLinkage, CodegenTarget, CodegenTargetBuildError,
    Endianness, ObjectFormat, RelocationModel, TargetAbi, TargetAbiBuildError, TargetAddressSpace,
    TargetAddressSpaceKind, TargetArchitecture, TargetCallingConvention, TargetCompatibility,
    TargetContract, TargetDataLayout, TargetDataLayoutBuildError, TargetIdentity,
    TargetMachineProperties, TargetMachineSelection, TargetScalarKind, TargetScalarLayout,
    TargetScalarLayoutBuildError, TargetSymbolConvention, TargetSymbolConventionBuildError,
};
pub use unit::{CodegenUnit, CodegenUnitBuildError, CodegenUnitKey};
