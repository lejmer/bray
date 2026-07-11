//! Semantic identities and canonical values for Bray programs.

#![forbid(unsafe_code)]

mod allocator;
mod build;
mod collection;
mod compiler_known;
mod error;
mod external;
mod fact;
mod graph;
mod id;
mod imported;
mod interface;
mod key;
mod kind;
mod local;
mod member;
mod name;
mod origin;
mod provider;
mod record;
mod relationship;
mod surface_kind;
mod value;

pub use compiler_known::{
    CompilerKnownDeclarationFact, CompilerKnownScopeSymbolId, CompilerKnownSymbolBuildError,
    CompilerKnownSymbolFactKey, CompilerKnownSymbolProvider,
};
pub use error::SymbolGraphBuildError;
pub use graph::{SymbolGraph, SymbolGraphRoots};

pub use external::{ExternalDeclarationIdentity, ExternalSymbolKey, ExternalSymbolKeyData};
pub use fact::{
    CallableContractClause, CallableContractClauseKind, CallableContractSet, CallableContractsFact,
    CallableParameterDefaultFact, CallableParameterDefaultSurface, CallableParameterDefaultValue,
    CallableParameterSignature, CallableSignature, CallableSignatureFact,
    CheckedCallableParameterDefault, CheckedConstraint, CheckedStructFieldDefault,
    CheckedUnionPayloadDefault, ConstantDeclaredTypeFact, ConstantDefinition,
    ConstantDefinitionFact, ConstantDefinitionState, ConstantInstanceKey,
    ConstantInstanceValueFact, ErrorCallableParameterDefault, ErrorConstantDefinition,
    ErrorPredicateDefinition, ErrorStructFieldDefault, ErrorUnionPayloadDefault,
    GenericConstraintSet, GenericConstraintsFact, ImplementationAmbiguity,
    ImplementationAmbiguityError, ImplementationCandidate, ImplementationSelection,
    ImplementationSelectionFact, ImplementationSelectionKey, ImplementationSubject,
    ImplementationSubjectFact, ImplementedTraitApplicationFact, NeverCancelSymbolCompletion,
    PredicateDefinition, PredicateDefinitionFact, PredicateDefinitionState,
    PredicateSemanticSummary, ReceiverParameterSignature, RuntimeDefaultBehavior,
    RuntimeDefaultCapabilityRequirement, RuntimeDefaultEffectRequirement,
    RuntimeDefaultGenericArguments, RuntimeDefaultGenericContext, RuntimeDefaultOwnership,
    RuntimeDefaultProviderInput, RuntimeDefaultTemplateReference, RuntimeDefaultTrustedObligation,
    SemanticFactContract, SemanticFactResult, StructFieldDefaultFact, StructFieldDefaultSurface,
    StructFieldDefaultValue, StructFieldTypeFact, SymbolCompletionCancellation,
    SymbolCompletionLevel, SymbolCompletionPlan, SymbolCompletionPlanError, SymbolCompletionUnit,
    SymbolFactCompletionRequest, SymbolFactContract, SymbolFactForcer, SymbolFactKind,
    SymbolFactRequest, SymbolFactResult, TraitConstantFulfillmentDeclaredTypeFact,
    TraitConstantFulfillmentDefinitionFact, TraitConstantMemberDeclaredTypeFact,
    TraitConstantMemberDefinitionFact, TraitPredicateFulfillmentDefinitionFact,
    TraitPredicateMemberDefinitionFact, TrustedCapabilityRequirement, UnionPayloadDefaultSurface,
    UnionPayloadDefaultValue, UnionPayloadFieldDefaultFact, UnionPayloadFieldTypeFact,
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
    ImportedSymbolFactKey, ImportedSymbolIdentity, ImportedSymbolIdentityInput,
    ImportedSymbolRelationship, ImportedSymbolSkeleton, ImportedSymbolSkeletonBuildError,
    ImportedSymbolSkeletonInput,
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
    PostconditionResultSymbol, PostconditionResultSymbolId,
};
pub use member::{
    MemberCollectionBuildError, MemberEntry, MemberLookupIndex, MemberLookupResult, MemberValidity,
    MemberVisibility, TypedMemberCollection,
};
pub use name::SymbolName;
pub use origin::SymbolOrigin;
pub use provider::{SymbolProvider, SymbolRecordId};
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
    CallableExecution, CallableInstanceData, CallableInstanceId, CallableParameterData,
    CallableParameterMode, CallableParameterName, CallablePosition, CallableTrust,
    CallableTypeData, ConcreteGenericSubstitutionId, ConstantBinaryOperation, ConstantProjection,
    ConstantProjectionKind, ConstantTermData, ConstantTermId, ConstantUnaryOperation,
    ConstantValueData, ConstantValueId, ConstantValueKind, DependencyContractTemplateData,
    DependencyContractTemplateId, DependencyGuard, DependencyProjection, DependencyRequirement,
    DependencyRequirementKind, DependencySubject, DependencySubjectRoot, GenericArgument,
    GenericArgumentKind, GenericBinding, GenericOwnerId, GenericSubstitutionData,
    GenericSubstitutionId, GenericSubstitutionShapeError, GuardedDependencyRequirement,
    ImplementationInstanceData, ImplementationInstanceId, IntegerConstant, IntegerSign,
    LifecycleObligationKind, RealConstantBits, SemanticValueKind, SemanticValueStore,
    SemanticValueStoreCreateError, SemanticValueStoreError, SemanticValueStoreId,
    TraitApplicationData, TraitApplicationId, TypeData, TypeId,
};
