//! Semantic identities and canonical values for Bray programs.

#![forbid(unsafe_code)]

mod allocator;
mod availability;
mod build;
mod collection;
mod compiler_known;
mod error;
mod external;
mod fact;
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

#[cfg(feature = "test-support")]
pub mod testing;

pub use compiler_known::{
    AvailableCompilerKnownSymbols, CompilerKnownCatalogAudit, CompilerKnownCatalogAuditError,
    CompilerKnownCatalogAuditReport, CompilerKnownDeclarationFact, CompilerKnownIterationProtocol,
    CompilerKnownOperationContract, CompilerKnownScopeSymbolId, CompilerKnownSymbolBuildError,
    CompilerKnownSymbolFactKey, CompilerKnownSymbolProvider, CompilerKnownSymbolRoleRegistry,
    CompilerKnownTargetProfile,
};
pub use error::SymbolGraphBuildError;
pub use graph::{SymbolGraph, SymbolGraphRoots};

pub use external::{ExternalDeclarationIdentity, ExternalSymbolKey, ExternalSymbolKeyData};
pub use fact::CallableSignatureTemplateError;
pub use fact::{
    CallableCapabilityRequirement, CallableContractClause, CallableContractClauseKind,
    CallableContractExpressionTemplate, CallableContractSet, CallableContractTemplate,
    CallableContractTemplateFact, CallableContractTypeFact, CallableContractsFact,
    CallableEffectRequirement, CallableExecutionRequirement, CallableOverloadTemplateFact,
    CallableParameterDefaultFact, CallableParameterDefaultSurface,
    CallableParameterDefaultTemplateFact, CallableParameterDefaultValue,
    CallableParameterSignature, CallableParameterTypeTemplate, CallablePhaseBehavior,
    CallableSignature, CallableSignatureFact, CallableSignatureTemplate, CallableTypeTemplate,
    CheckedCallableParameterDefault, CheckedConstraint, CheckedStructFieldDefault,
    CheckedUnionPayloadDefault, ConstantDeclaredTypeFact, ConstantDefinition,
    ConstantDefinitionFact, ConstantDefinitionState, ConstantExpressionExpectedType,
    ConstantExpressionOccurrence, ConstantExpressionOccurrenceKey, ConstantInstanceKey,
    ConstantInstanceValueFact, CurrentRunCancellation, DeclarationCapabilityTemplate,
    DeclarationExpressionTemplate, DeclarationPredicateClauseKind, ErrorCallableParameterDefault,
    ErrorConstantDefinition, ErrorPredicateDefinition, ErrorStructFieldDefault,
    ErrorUnionPayloadDefault, GenericArgumentTemplate, GenericConstParameterDeclaredTypeFact,
    GenericConstraintSet, GenericConstraintTemplate, GenericConstraintsFact,
    GenericDeclarationTemplate, GenericDeclarationTemplateFact, ImplementationAmbiguity,
    ImplementationAmbiguityError, ImplementationCandidate, ImplementationCandidateError,
    ImplementationCandidateSet, ImplementationCandidateSetError, ImplementationCandidateSetFact,
    ImplementationCoherenceDomainKey, ImplementationCoherenceEvidence,
    ImplementationCoherenceEvidenceError, ImplementationCoherenceFact, ImplementationCoherenceKey,
    ImplementationCoherenceParticipant, ImplementationHeadTemplate, ImplementationHeadTemplateFact,
    ImplementationOverloadTemplateFact, ImplementationParticipationEvidence,
    ImplementationParticipationFact, ImplementationParticipationKind,
    ImplementationParticipationSet, ImplementationParticipationSetError,
    ImplementationRequirementKey, ImplementationSelection, ImplementationSelectionCandidate,
    ImplementationSelectionFact, ImplementationSubject, ImplementationSubjectFact,
    ImplementationSubjectTemplate, ImplementedTraitApplicationFact, InherentTypeMemberValueFact,
    ModuleTargetGate, NeverCancelSymbolCompletion, OverloadArmTemplate, OverloadSignatureTemplate,
    ParticipatingImplementation, PredicateDefinition, PredicateDefinitionFact,
    PredicateDefinitionState, PredicateParameterTemplate, PredicateSemanticSummary,
    PredicateSignatureTemplate, PredicateSignatureTemplateFact, ReceiverMode,
    ReceiverParameterSignature, RuntimeDefaultBehavior, RuntimeDefaultCapabilityRequirement,
    RuntimeDefaultEffectRequirement, RuntimeDefaultGenericArguments, RuntimeDefaultGenericContext,
    RuntimeDefaultOwnership, RuntimeDefaultProviderInput, RuntimeDefaultTemplateReference,
    RuntimeDefaultTrustedObligation, SemanticFactContract, SemanticFactResult,
    SourceCallableContractTemplate, StructFieldDefaultFact, StructFieldDefaultSurface,
    StructFieldDefaultTemplateFact, StructFieldDefaultValue, StructFieldTypeFact,
    SymbolCompletionLevel, SymbolCompletionPlan, SymbolCompletionPlanError, SymbolCompletionUnit,
    SymbolFactCompletionRequest, SymbolFactContract, SymbolFactForcer, SymbolFactKind,
    SymbolFactRequest, SymbolFactResult, TargetFactDependency, TraitApplicationTemplate,
    TraitConstantFulfillmentDeclaredTypeFact, TraitConstantFulfillmentDefinitionFact,
    TraitConstantMemberDeclaredTypeFact, TraitConstantMemberDefinitionFact,
    TraitPredicateFulfillmentDefinitionFact, TraitPredicateMemberDefinitionFact,
    TraitTypeFulfillmentValueFact, TrustedCapabilityRequirement, TypeExpressionTemplate,
    UnevaluatedDefaultTemplate, UnionPayloadDefaultSurface, UnionPayloadDefaultValue,
    UnionPayloadFieldDefaultFact, UnionPayloadFieldDefaultTemplateFact, UnionPayloadFieldTypeFact,
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
    TraitTypeFulfillmentSymbolId, TraitTypeMemberSymbolId, TypeCallableMemberSymbolId,
    UnionPayloadDefaultProviderSymbolId, UnionPayloadFieldSymbolId, UnionSymbolId,
    UnionVariantSymbolId, UnnamedTraitImplementationSymbolId,
};
pub use imported::{
    ImportedIdentitySurfaceError, ImportedLookupEdge, ImportedPackageIdentitySurface,
    ImportedSymbolFactAddress, ImportedSymbolFactKey, ImportedSymbolIdentity,
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
pub use product::{ProductIdentity, ProductKind};
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
    TraitTypeMemberSymbol, TypeCallableMemberSymbol, UnionPayloadDefaultProviderSymbol,
    UnionPayloadFieldSymbol, UnionSymbol, UnionVariantSymbol, UnnamedTraitImplementationSymbol,
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
    IntegerConstant, IntegerSign, LifecycleObligationKind, RealConstantBits, SelfTypeContext,
    SemanticValueKind, SemanticValueStore, SemanticValueStoreCreateError, SemanticValueStoreError,
    SemanticValueStoreId, TargetSizedIntegerType, TraitApplicationData, TraitApplicationId,
    TypeData, TypeId,
};
