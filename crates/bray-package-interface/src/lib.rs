//! Deterministic wire encoding and bounded validation for compiled package interfaces.

#![forbid(unsafe_code)]

mod artifact;
mod construction;
mod decode;
mod diagnostic;
mod export;
mod hash;
mod header;
mod implementation;
mod inspection;
mod limits;
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
pub use diagnostic::InterfaceValidationError;
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
    ExecutableTemplateDecodeError, ExecutableTemplateEncodeContext, ExecutableTemplateEncodeError,
    InterfaceConstantCallableBody, InterfaceExecutableTemplate, InterfaceNativeBoundary,
    PackageImplementationArtifact, PackageImplementationArtifactBuildError,
    decode_executable_template, encode_executable_template,
};
pub use inspection::{
    InterfaceInspectionRecord, InterfaceInspectionRecordKind, InterfaceInspectionSection,
    InterfaceSectionIndexEntry, PackageInterfaceInspection,
};
pub use limits::{InterfaceLimit, InterfaceValidationLimits, InterfaceValidationPolicy};
pub use section::{InterfaceSectionTag, ValidatedInterfaceSection};
pub use semantic::{
    EncodedSemanticSection, ImportedAbiDependency, ImportedCallableContractFact,
    ImportedCallableParameterDefaultFact, ImportedCallableSignatureFact, ImportedConstraintFact,
    ImportedDeclarationTemplateFact, ImportedDeclaredTypeFact, ImportedGenericDeclarationFact,
    ImportedImplementationFact, ImportedPredicateDefinitionFact, ImportedRuntimeRequirement,
    ImportedSemanticFact, ImportedSemanticFacts, ImportedSourceProvenance, ImportedTargetFact,
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
    InterfaceSemanticFactEntry, InterfaceSemanticFactKind, InterfaceSemanticFacts,
    InterfaceSemanticInternError, InterfaceSourceProvenance, InterfaceStorageMember,
    InterfaceStorageShape, InterfaceSupportEntity, InterfaceSupportImplementation,
    InterfaceSymbolResolver, InterfaceTargetFactDependency, InterfaceTemplateReference,
    InterfaceTraitApplication, InterfaceTraitApplicationId, InterfaceTrustedCapabilityRequirement,
    InterfaceType, InterfaceTypeId, InterfaceTypeRepresentation, InterfaceUnionStorageVariant,
    InterfaceUnionTag, decode_semantic_facts, encode_semantic_facts,
};
pub use surface::{
    CompilerKnownSymbolReference, DependencyInterfaceId, ExportedLookupEdge, ExportedLookupKind,
    InterfaceDependency, InterfaceProductIdentity, InterfaceProductKind, InterfaceSymbolReference,
    PackageInterfaceIdentity, PackageInterfaceSurface, PackageInterfaceSurfaceBuildError,
    SymbolRelationship, SymbolRelationshipKind,
};
pub use validation::ValidatedPackageInterface;
