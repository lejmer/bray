//! Semantic identities and canonical values for Bray programs.

#![forbid(unsafe_code)]

mod allocator;
mod availability;
mod build;
mod collection;
mod compiler_known;
mod diagnostic;
mod error;
mod external;
mod semantic;
mod graph;
mod id;
mod imported;
mod inference;
mod interface;
mod key;
mod kind;
mod local;
mod member;
mod name;
mod observation;
mod origin;
mod product;
mod provider;
mod recognized;
mod record;
mod relationship;
mod runtime_default;
mod surface_kind;
#[cfg(test)]
mod test_support;
mod value;
mod version;

#[cfg(feature = "test-support")]
pub mod testing;

pub use compiler_known::{
    AvailableCompilerKnownSymbols, CompilerKnownCatalogAudit, CompilerKnownCatalogAuditError,
    CompilerKnownCatalogAuditReport, CompilerKnownDeclarationSemantics, CompilerKnownIterationProtocol,
    CompilerKnownOperationContract, CompilerKnownOrderingRepresentation,
    CompilerKnownResultRepresentation, CompilerKnownRunResultRepresentation,
    CompilerKnownScopeSymbolId, CompilerKnownSymbolBuildError, CompilerKnownSemanticKey,
    CompilerKnownSymbolProvider, CompilerKnownSymbolRoleRegistry, CompilerKnownTargetProfile,
};
pub use diagnostic::{
    diagnostic_callable_abi, diagnostic_callable_execution, diagnostic_symbol_identity,
    diagnostic_symbol_kind,
};
pub use error::SymbolGraphBuildError;
pub use graph::{SymbolGraph, SymbolGraphRoots};

pub use external::{ExternalDeclarationIdentity, ExternalSymbolKey, ExternalSymbolKeyData};
pub use semantic::CallableSignatureTemplateError;
pub use semantic::{
    CallableCapabilityRequirement, CallableContractClause, CallableContractClauseKind,
    CallableContractClauseValue, CallableContractExpressionTemplate, CallableContractSet,
    CallableContractTemplate, CallableContractTemplateQuery, CallableContractTypeQuery,
    CallableContractsQuery, CallableEffectRequirement, CallableExecutionRequirement,
    CallableOverloadTemplateQuery, CallableParameterDefaultQuery, CallableParameterDefaultSurface,
    CallableParameterDefaultTemplateQuery, CallableParameterDefaultValue,
    CallableParameterSignature, CallableParameterTypeTemplate, CallablePhaseBehavior,
    CallablePhaseBehaviors, CallableSignature, CallableSignatureQuery, CallableSignatureTemplate,
    CallableTypeDirectiveKey, CallableTypeTemplate, CheckedCallableParameterDefault,
    CheckedConstraint, CheckedConstraintKind, CheckedStructFieldDefault,
    CheckedUnionPayloadDefault, ConstantDeclaredTypeQuery, ConstantDefinition,
    ConstantDefinitionQuery, ConstantDefinitionState, ConstantExpressionExpectedType,
    ConstantExpressionOccurrence, ConstantExpressionOccurrenceKey, ConstantInstanceKey,
    CurrentRunCancellation, DeclarationCapabilityTemplate, DeclarationDirectivesQuery,
    DeclarationExpressionTemplate, DeclarationPredicateClauseKind, DeclaredCopyContract,
    DeclaredLayoutMode, DeclaredStorageShape, DeclaredStructStorageMember,
    DeclaredTypeRepresentation, DeclaredUnionStorageMember, DeclaredUnionStorageVariant,
    DeclaredUnionTag, DirectiveArgumentName, DirectiveArgumentTemplate, DirectiveAttachment,
    DirectiveKind, DirectiveSurface, DirectiveTemplate, ErrorCallableParameterDefault,
    ErrorConstantDefinition, ErrorPredicateDefinition, ErrorStructFieldDefault,
    ErrorUnionPayloadDefault, ForeignCallableContract, ForeignCallableDirection,
    GenericArgumentTemplate, GenericConstParameterDeclaredTypeQuery, GenericConstraintObligationKey,
    GenericConstraintSatisfactionQuery, GenericConstraintSet, GenericConstraintTemplate,
    GenericConstraintsQuery, GenericDeclarationTemplate, GenericDeclarationTemplateQuery,
    ImplementationAmbiguity, ImplementationAmbiguityError, ImplementationCandidate,
    ImplementationCandidateError, ImplementationCandidateSet, ImplementationCandidateSetError,
    ImplementationCandidateSetQuery, ImplementationCoherenceDomainKey,
    ImplementationCoherenceEvidence, ImplementationCoherenceEvidenceError,
    ImplementationCoherenceQuery, ImplementationCoherenceKey, ImplementationCoherenceParticipant,
    ImplementationHeadTemplate, ImplementationHeadTemplateQuery, ImplementationOverloadTemplateQuery,
    ImplementationParticipationEvidence, ImplementationParticipationQuery,
    ImplementationParticipationKind, ImplementationParticipationSet,
    ImplementationParticipationSetError, ImplementationRequirementKey, ImplementationSelection,
    ImplementationSelectionCandidate, ImplementationSelectionQuery, ImplementationSubject,
    ImplementationSubjectQuery, ImplementationSubjectTemplate, ImplementedTraitApplicationQuery,
    InherentTypeMemberValueQuery, ModuleContributionGate, ModuleReExport, ModuleSurface,
    ModuleSurfaceQuery, ModuleUsing, NativeLinkKind, NativeLinkRequirement,
    NeverCancelSymbolCompletion, OverloadArmTemplate, OverloadSignatureTemplate,
    ParticipatingImplementation, PredicateDefinition, PredicateDefinitionQuery,
    PredicateDefinitionState, PredicateParameterTemplate, PredicateSemanticSummary,
    PredicateSignatureTemplate, PredicateSignatureTemplateQuery, ProofOutcome, ReceiverMode,
    ReceiverParameterSignature, RuntimeDefaultBehavior, RuntimeDefaultCapabilityRequirement,
    RuntimeDefaultEffectRequirement, RuntimeDefaultGenericArguments, RuntimeDefaultGenericContext,
    RuntimeDefaultOwnership, RuntimeDefaultProviderInput, RuntimeDefaultTemplateReference,
    RuntimeDefaultTrustedObligation, SemanticFactContract, SemanticFactResult,
    SourceCallableContractTemplate, StructFieldDefaultQuery, StructFieldDefaultSurface,
    StructFieldDefaultTemplateQuery, StructFieldDefaultValue, StructFieldTypeQuery,
    SymbolCompletionLevel, SymbolCompletionPlan, SymbolCompletionPlanError, SymbolCompletionUnit,
    SymbolFactCompletionRequest, SymbolFactContract, SymbolFactForcer, SymbolFactKind,
    SymbolFactRequest, SymbolFactResult, TargetPropertyDependency, TraitApplicationTemplate,
    TraitConstantFulfillmentDeclaredTypeQuery, TraitConstantFulfillmentDefinitionQuery,
    TraitConstantMemberDeclaredTypeQuery, TraitConstantMemberDefinitionQuery,
    TraitConstraintDispatch, TraitImplementationConformance, TraitImplementationConformanceQuery,
    TraitMemberFulfillmentId, TraitMemberRequirementId, TraitPredicateFulfillmentDefinitionQuery,
    TraitPredicateMemberDefinitionQuery, TraitRequirementConformance, TraitRequirementResolution,
    TraitTypeFulfillmentValueQuery, TrustedCapabilityRequirement, TypeAssociatedImplementation,
    TypeAssociatedLifecycleMember, TypeAssociatedLifecycleSlot, TypeAssociatedMember,
    TypeAssociatedMemberOrigin, TypeAssociatedSurface, TypeAssociatedSurfaceBuildError,
    TypeExpressionTemplate, UnevaluatedDefaultTemplate, UnionPayloadDefaultSurface,
    UnionPayloadDefaultValue, UnionPayloadFieldDefaultQuery, UnionPayloadFieldDefaultTemplateQuery,
    UnionPayloadFieldTypeQuery,
};
pub use id::{
    AnySymbolId, CallableContractSymbolId, CallableOverloadSymbolId,
    CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId, CallableSymbolId,
    CompilerKnownEnvironmentSymbolId, ConstantSymbolId, ConstructorSymbolId, DestructorSymbolId,
    ExactSymbolId, FinalizerSymbolId, FunctionSymbolId, GenericConstParameterSymbolId,
    GenericParameterSymbolId, GenericTypeParameterSymbolId, ImplementationOverloadSymbolId,
    ImplementationSymbolId, InherentImplementationSymbolId, InherentTypeMemberSymbolId,
    ModuleOwnerId, ModuleSymbolId, NamedTraitImplementationSymbolId, NamedTypeSymbolId,
    PackageSymbolId, ParameterSymbolId, PredicateDefinitionSymbolId, PredicateParameterSymbolId,
    PredicateSymbolId, ReceiverParameterSymbolId, ScopeEnterSymbolId, ScopeExitSymbolId,
    StructFieldDefaultProviderSymbolId, StructFieldSymbolId, StructSymbolId, SymbolId,
    SymbolRootId, TraitCallableFulfillmentSymbolId, TraitCallableMemberSymbolId,
    TraitConstantFulfillmentSymbolId, TraitConstantMemberSymbolId,
    TraitDestructorRequirementSymbolId, TraitFinalizerRequirementSymbolId,
    TraitPredicateFulfillmentSymbolId, TraitPredicateMemberSymbolId,
    TraitScopeEnterFulfillmentSymbolId, TraitScopeEnterRequirementSymbolId,
    TraitScopeExitFulfillmentSymbolId, TraitScopeExitRequirementSymbolId, TraitSymbolId,
    TraitTypeFulfillmentSymbolId, TraitTypeMemberSymbolId, TrustedCapabilitySymbolId,
    TypeCallableMemberSymbolId, UnionPayloadDefaultProviderSymbolId, UnionPayloadFieldSymbolId,
    UnionSymbolId, UnionVariantSymbolId, UnnamedTraitImplementationSymbolId,
};
pub use imported::{
    ImportedIdentitySurfaceError, ImportedLookupEdge, ImportedPackageIdentitySurface,
    ImportedSemanticAddress, ImportedSemanticKey, ImportedSymbolIdentity,
    ImportedSymbolIdentityInput, ImportedSymbolRelationship, ImportedSymbolSkeleton,
    ImportedSymbolSkeletonBuildError, ImportedSymbolSkeletonInput,
};
pub use interface::{ImportedInterfaceId, InterfaceSupportEntityId, InterfaceSymbolId};
pub use key::{
    ModulePathKey, PackageIdentity, SymbolKey, SymbolKeyData, SymbolOrdinal, SymbolRootKey,
    SynthesizedSymbolKey, SynthesizedSymbolRole,
};
pub use kind::{SymbolKind, SymbolRelationshipKind};
pub use local::{
    AnonymousCallableParameterSymbol, AnonymousCallableParameterSymbolId, AnonymousCallableSymbol,
    AnonymousCallableSymbolId, AnyLocalSymbolId, LocalBindingSymbol, LocalBindingSymbolId,
    LocalConstantSymbol, LocalConstantSymbolId, LocalScope, LocalScopeBoundary, LocalScopeId,
    LocalSymbolBuildError, LocalSymbolKey, LocalSymbolRegionId, LocalSymbolRegionKey,
    LocalSymbolRegionRole, LocalSymbolSnapshot, LocalSymbolSnapshotBuilder,
    LocalSymbolSnapshotCheckpoint, PostconditionResultSymbol, PostconditionResultSymbolId,
};
pub use member::{
    MemberCollectionBuildError, MemberEntry, MemberLookupIndex, MemberLookupResult, MemberValidity,
    MemberVisibility, TypedMemberCollection,
};
pub use name::SymbolName;
pub use origin::SymbolOrigin;
pub use product::{
    ProductIdentity, ProductKind, ProductSemantics, ProductTestEntry, TestExecutionConstraint,
    TestResultShape,
};
pub use provider::{SymbolProvider, SymbolRecordId};
pub use recognized::{
    RecognizedStandardLibraryDeclarationMatch, RecognizedStandardLibraryDeclarations,
};
pub use record::{
    CallableContractSymbol, CallableOverloadSymbol, CallableParameterDefaultProviderSymbol,
    CallableParameterSymbol, CompilerKnownEnvironmentSymbol, ConstantSymbol, ConstructorSymbol,
    DestructorSymbol, FinalizerSymbol, FunctionSymbol, GenericConstParameterSymbol,
    GenericTypeParameterSymbol, ImplementationOverloadSymbol, InherentImplementationSymbol,
    InherentTypeMemberSymbol, ModuleSymbol, NamedTraitImplementationSymbol, PackageSymbol,
    PredicateParameterSymbol, PredicateSymbol, ReceiverParameterSymbol, ScopeEnterSymbol,
    ScopeExitSymbol, StructFieldDefaultProviderSymbol, StructFieldSymbol, StructSymbol,
    TraitCallableFulfillmentSymbol, TraitCallableMemberSymbol, TraitConstantFulfillmentSymbol,
    TraitConstantMemberSymbol, TraitDestructorRequirementSymbol, TraitFinalizerRequirementSymbol,
    TraitPredicateFulfillmentSymbol, TraitPredicateMemberSymbol, TraitScopeEnterFulfillmentSymbol,
    TraitScopeEnterRequirementSymbol, TraitScopeExitFulfillmentSymbol,
    TraitScopeExitRequirementSymbol, TraitSymbol, TraitTypeFulfillmentSymbol,
    TraitTypeMemberSymbol, TrustedCapabilitySymbol, TypeCallableMemberSymbol,
    UnionPayloadDefaultProviderSymbol, UnionPayloadFieldSymbol, UnionSymbol, UnionVariantSymbol,
    UnnamedTraitImplementationSymbol,
};
pub use relationship::RuntimeDefaultPresence;
pub use value::{
    AnyConstantDefinitionId, BorrowKind, CallableAbi, CallableConstness, CallableDefinitionId,
    CallableDependencyContracts, CallableExecution, CallableInstanceData, CallableInstanceId,
    CallableParameterData, CallableParameterMode, CallableParameterName, CallablePosition,
    CallableTrust, CallableTypeData, ConcreteGenericSubstitutionId, ConstantBinaryOperation,
    ConstantField, ConstantProjection, ConstantProjectionKind, ConstantTermData, ConstantTermId,
    ConstantUnaryOperation, ConstantValueData, ConstantValueId, ConstantValueKind,
    DependencyContractTemplateData, DependencyContractTemplateId, DependencyGuard,
    DependencyProjection, DependencyRequirement, DependencyRequirementKind, DependencySubject,
    DependencySubjectRoot, GenericArgument, GenericArgumentKind, GenericBinding, GenericOwnerId,
    GenericSubstitutionData, GenericSubstitutionId, GenericSubstitutionShapeError,
    GuardedDependencyRequirement, ImplementationInstanceData, ImplementationInstanceId,
    IntegerConstant, IntegerSign, LifecycleObligationKind, PredicateInstanceData, RealConstantBits,
    SelfTypeContext, SemanticValueKind, SemanticValueStore, SemanticValueStoreCreateError,
    SemanticValueStoreError, SemanticValueStoreId, TargetSizedIntegerType, TraitApplicationData,
    TraitApplicationId, TypeData, TypeId,
};
pub use version::PackageVersion;
