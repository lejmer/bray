//! Backend-independent code generation orchestration.

#![forbid(unsafe_code)]

mod artifact;
mod backend;
mod capability;
mod mapping;
mod optimization;
mod options;
mod outcome;
mod registry;
mod request;
mod target;
mod unit;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use artifact::{
    ArtifactContent, ArtifactContentBuildError, ArtifactContentReader, ArtifactContentSource,
    ArtifactDigest, ArtifactDigestAlgorithm, ArtifactSpool, ArtifactSpoolError,
    ArtifactSpoolOperation, ArtifactSpoolWriter, AssemblySyntaxKind, BackendArtifactContribution,
    BackendArtifactId, BackendArtifactKind, BackendArtifactRequest, BackendArtifactRequestEntry,
    BackendArtifactRequirement,
    BackendBitcodeSemantics,
    BackendSerializationOptions, DebugInformationOutputMode, LinkableArtifactKind,
    LinkableArtifactRequirement,
};
pub use backend::{BackendIdentity, CodeGenerator};
pub use capability::{
    BackendCapabilities, BackendCapabilityRevision, BackendOptimizationCapabilities,
    BackendOutputCapabilities, BackendRuntimeCapabilities, BackendTargetCapabilities,
    BackendTargetConfiguration, ReproducibilityLevel,
};
pub use mapping::{
    CodegenCallSite, CodegenCallableMapping, CodegenCallableSignature, CodegenCallableTarget,
    CodegenConstantMapping, CodegenConstantTermMapping, CodegenDebugLocation, CodegenFieldLayout,
    CodegenHelperMapping, CodegenIndirectParameterKind, CodegenInstanceTypeMapping,
    CodegenIntegerExtension, CodegenMappings, CodegenNativeEntryMapping,
    CodegenNativeStaticMapping, CodegenOperationMapping, CodegenParameterMapping,
    CodegenProductHostMapping, CodegenProductHostStatic, CodegenResultMapping, CodegenSourceFile,
    CodegenStaticFinalization, CodegenStaticIncidentMemory, CodegenStaticInstanceKey,
    CodegenStaticRelocation, CodegenStaticStorageMapping, CodegenStaticWitness, CodegenSymbolKey,
    CodegenSymbolMapping, CodegenTerminatorMapping, CodegenTypeBehavior, CodegenTypeKind,
    CodegenTypeMapping, CodegenUnionVariantLayout, CodegenValueAttribute, ConstantDemands,
    DemandedCallableInstance, FOREIGN_CALLBACK_RUNTIME_ROLES, IntrinsicCall, child_constants,
    demanded_callable_instance_for_call, demanded_callable_instances,
    demanded_callable_instances_for_mir, demanded_constant_terms, demanded_constants,
    demanded_debug_sources, demanded_runtime_references, demanded_runtime_references_for_mir,
    mapped_runtime_references, static_host_section_name,
};
pub use optimization::{
    BackendBitcodeOptimizationOutcome, BackendBitcodeOptimizer, BackendBitcodeTargetContract,
};
pub use options::{
    CodegenOptions, DebugInformationMode, OptimizationLevel, RuntimeObservationMode, SizePreference,
};
pub use outcome::{
    CodegenFailure, CodegenOutcome, CodegenStatus,
    codegen_failure_diagnostic, codegen_failure_diagnostics,
};
pub use registry::{
    BackendSelectionError, CodeGeneratorRegistry, CodeGeneratorRegistryBuildError,
    CodegenConfiguration,
};
pub use request::CodegenRequest;
pub use target::{
    CallableAbiMapping, CodegenLinkage, CodegenTarget, CodegenTargetBuildError, TargetAbi,
    TargetAbiBuildError, TargetAddressSpace, TargetAddressSpaceKind, TargetCallingConvention,
    TargetCompatibility, TargetContract, TargetDataLayout, TargetDataLayoutBuildError,
    TargetMachineSelection, TargetScalarKind, TargetScalarLayout, TargetScalarLayoutBuildError,
    TargetSymbolConvention, TargetSymbolConventionBuildError,
};
pub use unit::{
    CodegenDefinitionVisibility, CodegenGenericArgument, CodegenImplementationWitness,
    CodegenInstance, CodegenInstanceBuildError, CodegenInstanceDependency,
    CodegenInstanceDependencyKind, CodegenInstanceKey, CodegenOversizedUnit,
    CodegenOversizedUnitReason, CodegenPartitionCompatibility, CodegenPartitionError,
    CodegenPartitionPolicy, CodegenPartitionPolicyBuildError, CodegenReachability,
    CodegenReachabilityBuildError, CodegenReachabilityBuilder, CodegenSpecialization, CodegenUnit,
    CodegenUnitBuildError, CodegenUnitKey, CodegenValueKey, CodegenWork, partition_codegen_units,
};
