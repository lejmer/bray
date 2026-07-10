//! Semantic identities for declared Bray program items.

#![forbid(unsafe_code)]

mod build;
mod collection;
mod error;
mod graph;
mod id;
mod key;
mod kind;
mod origin;
mod record;

pub use error::SymbolGraphBuildError;
pub use graph::{SymbolGraph, SymbolGraphRoots};

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
pub use record::{
    CallableContractSymbol, CallableOverloadSymbol, CallableParameterSymbol,
    CompilerKnownEnvironmentSymbol, ConstantSymbol, ConstructorSymbol, DestructorSymbol,
    FinalizerSymbol, FunctionSymbol, GenericConstParameterSymbol, GenericTypeParameterSymbol,
    ImplementationOverloadSymbol, InherentImplementationSymbol, InherentTypeMemberSymbol,
    ModuleSymbol, NamedTraitImplementationSymbol, PackageSymbol, PredicateParameterSymbol,
    PredicateSymbol, ScopeEnterSymbol, ScopeExitSymbol, StructFieldSymbol, StructSymbol,
    TraitCallableFulfillmentSymbol, TraitCallableMemberSymbol, TraitConstantFulfillmentSymbol,
    TraitConstantMemberSymbol, TraitDestructorRequirementSymbol, TraitFinalizerRequirementSymbol,
    TraitPredicateFulfillmentSymbol, TraitPredicateMemberSymbol, TraitScopeEnterFulfillmentSymbol,
    TraitScopeEnterRequirementSymbol, TraitScopeExitFulfillmentSymbol,
    TraitScopeExitRequirementSymbol, TraitSymbol, TraitTypeFulfillmentSymbol,
    TraitTypeMemberSymbol, TypeCallableMemberSymbol, UnionPayloadFieldSymbol, UnionSymbol,
    UnionVariantSymbol, UnnamedTraitImplementationSymbol,
};
