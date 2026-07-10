//! Semantic identities and canonical values for Bray programs.

#![forbid(unsafe_code)]

mod id;
mod key;
mod kind;
mod origin;
mod value;

pub use id::{
    AnySymbolId, CallableContractSymbolId, CallableOverloadSymbolId,
    CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId,
    CompilerKnownEnvironmentSymbolId, ConstantSymbolId, ConstructorSymbolId, DestructorSymbolId,
    FinalizerSymbolId, FunctionSymbolId, GenericConstParameterSymbolId, GenericParameterSymbolId,
    GenericTypeParameterSymbolId, ImplementationOverloadSymbolId, ImplementationSymbolId,
    InherentImplementationSymbolId, InherentTypeMemberSymbolId, ModuleOwnerId, ModuleSymbolId,
    NamedTraitImplementationSymbolId, NamedTypeSymbolId, PackageSymbolId, ParameterSymbolId,
    PredicateParameterSymbolId, PredicateSymbolId, ReceiverParameterSymbolId, ScopeEnterSymbolId,
    ScopeExitSymbolId, StructFieldDefaultProviderSymbolId, StructFieldSymbolId, StructSymbolId,
    SymbolId, SymbolRootId, TraitCallableFulfillmentSymbolId, TraitCallableMemberSymbolId,
    TraitConstantFulfillmentSymbolId, TraitConstantMemberSymbolId,
    TraitDestructorRequirementSymbolId, TraitFinalizerRequirementSymbolId,
    TraitPredicateFulfillmentSymbolId, TraitPredicateMemberSymbolId,
    TraitScopeEnterFulfillmentSymbolId, TraitScopeEnterRequirementSymbolId,
    TraitScopeExitFulfillmentSymbolId, TraitScopeExitRequirementSymbolId, TraitSymbolId,
    TraitTypeFulfillmentSymbolId, TraitTypeMemberSymbolId, TypeCallableMemberSymbolId,
    UnionPayloadDefaultProviderSymbolId, UnionPayloadFieldSymbolId, UnionSymbolId,
    UnionVariantSymbolId, UnnamedTraitImplementationSymbolId,
};
pub use key::{
    ModulePathKey, PackageIdentity, SymbolKey, SymbolKeyData, SymbolOrdinal, SymbolRootKey,
    SynthesizedSymbolKey, SynthesizedSymbolRole,
};
pub use kind::SymbolKind;
pub use origin::SymbolOrigin;
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
