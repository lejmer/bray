//! Backend-independent code generation orchestration.

#![forbid(unsafe_code)]

mod artifact;
mod backend;
mod mapping;
mod options;
mod outcome;
mod request;
mod registry;
mod runtime;
mod target;
mod unit;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

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
pub use mapping::{
    CodegenCallableMapping, CodegenCallableSignature, CodegenConstantMapping,
    CodegenConstantTermMapping, CodegenDebugLocation, CodegenFieldLayout, CodegenHelperMapping,
    CodegenIndirectParameterKind, CodegenIntegerExtension, CodegenMappings,
    CodegenMappingsBuildError, CodegenOperationMapping, CodegenParameterMapping,
    CodegenResultMapping, CodegenSourceFile, CodegenSymbolKey, CodegenSymbolMapping,
    CodegenTerminatorMapping, CodegenTypeKind, CodegenTypeMapping, CodegenUnionVariantLayout,
    CodegenValueAttribute, demanded_runtime_references,
};
pub use options::{CodegenOptions, DebugInformationMode, OptimizationLevel, SizePreference};
pub use outcome::{CodegenFailure, CodegenOutcome, CodegenStatus};
pub use request::{CodegenRequest, CodegenRequestBuildError};
pub use registry::{
    BackendSelectionError, CodeGeneratorRegistry, CodeGeneratorRegistryBuildError,
    CodegenConfiguration,
};
pub use runtime::{
    CodegenRuntimeMetadata, CodegenRuntimeMetadataBuildError, ProtectedAsyncFrameMetadata,
};
pub use target::{
    CallableAbiMapping, CodegenLinkage, CodegenTarget, CodegenTargetBuildError, TargetAbi,
    TargetAbiBuildError, TargetAddressSpace, TargetAddressSpaceKind, TargetCallingConvention,
    TargetCompatibility, TargetContract, TargetDataLayout, TargetDataLayoutBuildError,
    TargetMachineSelection, TargetScalarKind, TargetScalarLayout, TargetScalarLayoutBuildError,
    TargetSymbolConvention, TargetSymbolConventionBuildError,
};
pub use unit::{
    CodegenGenericArgument, CodegenImplementationWitness, CodegenInstance,
    CodegenInstanceBuildError, CodegenInstanceDependency, CodegenInstanceDependencyKind,
    CodegenInstanceKey, CodegenReachability, CodegenReachabilityBuildError,
    CodegenReachabilityBuilder, CodegenSpecialization, CodegenUnit, CodegenUnitBuildError,
    CodegenUnitKey, CodegenValueKey, partition_codegen_units,
};
