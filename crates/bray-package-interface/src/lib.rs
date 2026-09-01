//! Deterministic wire encoding and bounded validation for compiled package interfaces.

#![forbid(unsafe_code)]

mod artifact;
mod construction;
mod decode;
mod diagnostic;
mod encoding;
mod export;
mod external_key;
mod framing;
mod hash;
mod header;
mod implementation;
mod inspection;
mod limits;
mod presentation;
mod section;
mod semantic;
mod surface;
mod tag;
mod validation;
mod wire;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use artifact::{InterfaceArtifact, InterfaceArtifactIntegrityError};
pub use construction::{
    ImportedInterfaceSymbolResolver, ImportedSymbolConstructionError, LoadedInterfaceSurface,
    construct_imported_symbol_skeletons,
};
pub use diagnostic::{
    InterfaceCompressionFailure, InterfaceIntegerTarget, InterfaceMalformedCause,
    InterfaceUtf8Failure, InterfaceValidationContext, InterfaceValidationError,
    InterfaceValidationField,
};
pub use encoding::InterfaceSectionEncoding;
pub use export::{
    ExportLookupInput, ExportRelationshipInput, ExportSymbolInput, ExportSymbolReferenceInput,
    PackageInterfaceExportBuildError, PackageInterfaceExportBundle,
    PackageInterfaceExportSurfaceError, build_package_interface_surface, encode_package_interface,
};
pub use hash::{InterfaceArtifactHash, InterfaceContentHash, InterfaceSectionHash};
pub use header::{
    CURRENT_FORMAT_REVISION, InterfaceFormatRevision, InterfaceHeader, InterfaceLanguageRevision,
    InterfaceRequiredFlags,
};
pub use implementation::{
    CURRENT_MIR_SCHEMA_REVISION, CURRENT_TEMPLATE_SCHEMA_REVISION, ExecutableTemplateDecodeError,
    ExecutableTemplateEncodeContext, ExecutableTemplateEncodeError,
    ImplementationExternalSymbolIdentity, ImplementationMirSchemaRevision,
    ImplementationSpecializationArgument, ImplementationSpecializationArgumentKind,
    ImplementationSpecializationWitness, ImplementationTemplateSchemaRevision,
    InterfaceConstantCallableBody, InterfaceExecutableTemplate, InterfaceNativeBoundary,
    InterfaceNativeBoundaryKind, InterfacePreSpecializedMir, PackageImplementationArtifact,
    PackageImplementationArtifactBuildError, PackageImplementationConfiguration,
    PackageImplementationIdentity, PackageImplementationSpecializationKey,
    PackageImplementationTargetProperties, PackageImplementationTargetProperty,
    PackageImplementationTargetPropertyValue, PreSpecializedMirDecodeError,
    decode_executable_template, encode_executable_template, encode_pre_specialized_mir,
};
pub use inspection::{
    InterfaceInspectionRecord, InterfaceInspectionRecordKind, InterfaceInspectionSection,
    InterfaceSectionIndexEntry, PackageInterfaceInspection,
};
pub use limits::{InterfaceLimit, InterfaceValidationLimits, InterfaceValidationPolicy};
pub use section::{
    InterfaceSectionCompatibility, InterfaceSectionRevision, InterfaceSectionTag,
    ValidatedInterfaceSection,
};
pub use semantic::{
    EncodedSemanticSection, ImportedAbiDependency, ImportedCallableContract,
    ImportedCallableParameterDefault, ImportedCallableSignature, ImportedConstraint,
    ImportedDeclarationTemplate, ImportedDeclaredType, ImportedGenericDeclaration,
    ImportedImplementation, ImportedPredicateDefinition, ImportedRuntimeRequirement,
    ImportedSemanticRecord, ImportedSemantics, ImportedSourceProvenance, ImportedTargetProperty,
    InterfaceAbiDependency, InterfaceCallableContract, InterfaceCallableContractClause,
    InterfaceCallableContractClauseValue, InterfaceCallableInstance, InterfaceCallableInstanceId,
    InterfaceCallableParameter, InterfaceCallableParameterDefault, InterfaceCallablePhaseBehavior,
    InterfaceCallableReceiver, InterfaceCallableSignature, InterfaceCheckedTemplate,
    InterfaceCheckedTemplateBehavior, InterfaceCheckedTemplateExecution,
    InterfaceCheckedTemplateId, InterfaceCheckedTemplateInput, InterfaceCheckedTemplateInputKind,
    InterfaceCheckedTemplateNode, InterfaceCheckedTemplateOperation,
    InterfaceCheckedTemplateTemporary, InterfaceCoherenceRecord, InterfaceConstantProjection,
    InterfaceConstantTerm, InterfaceConstantTermId, InterfaceConstantValue,
    InterfaceConstantValueId, InterfaceConstantValueKind, InterfaceConstraint,
    InterfaceConstraintKind, InterfaceDeclarationTemplate, InterfaceDeclaredType,
    InterfaceDependencyContract, InterfaceDependencyContractId, InterfaceDependencyGuard,
    InterfaceDependencyProjection, InterfaceDependencyRequirement,
    InterfaceDependencyRequirementKind, InterfaceDependencyRequirementValue,
    InterfaceDependencySubject, InterfaceDependencySubjectRoot, InterfaceGenericArgument,
    InterfaceGenericBinding, InterfaceGenericDeclaration, InterfaceGenericSubstitution,
    InterfaceGenericSubstitutionId, InterfaceImplementationInstance,
    InterfaceImplementationInstanceId, InterfaceImplementationRecord,
    InterfaceImplementationReference, InterfacePredicateDefinition,
    InterfacePredicateDefinitionState, InterfacePredicateSummary, InterfaceRuntimeRequirement,
    InterfaceSemanticCommitError, InterfaceSemanticIdRemap, InterfaceSemanticInternError,
    InterfaceSemanticRecord, InterfaceSemanticRecordKind, InterfaceSemanticTableKind,
    InterfaceSemantics, InterfaceSourceProvenance, InterfaceStorageMember, InterfaceStorageShape,
    InterfaceSupportEntity, InterfaceSupportImplementation, InterfaceSymbolResolver,
    InterfaceTargetPropertyDependency, InterfaceTemplateReference, InterfaceTraitApplication,
    InterfaceTraitApplicationId, InterfaceTrustedCapabilityRequirement, InterfaceType,
    InterfaceTypeId, InterfaceTypeRepresentation, InterfaceUnionStorageVariant, InterfaceUnionTag,
    commit_interface_semantic_fragments, decode_semantics, encode_semantics,
};
pub use surface::{
    CompilerKnownSymbolReference, DependencyInterfaceId, ExportedLookupEdge, ExportedLookupKind,
    InterfaceDependency, InterfaceProductIdentity, InterfaceProductKind, InterfaceSymbolReference,
    PackageInterfaceIdentity, PackageInterfaceSurface, PackageInterfaceSurfaceBuildError,
    SymbolRelationship, SymbolRelationshipKind,
};
pub use validation::ValidatedPackageInterface;
