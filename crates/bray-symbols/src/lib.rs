//! Semantic identities for declared Bray program items.

#![forbid(unsafe_code)]

mod external;
mod id;
mod imported;
mod interface;
mod key;
mod kind;
mod origin;

pub use external::{
    ExternalDeclarationIdentity, ExternalSymbolKey, ExternalSymbolKeyData, ExternalSymbolName,
};
pub use id::{
    AnySymbolId, CallableContractSymbolId, CallableOverloadSymbolId,
    CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId,
    CompilerKnownEnvironmentSymbolId, ConstantSymbolId, ConstructorSymbolId, DestructorSymbolId,
    ExactSymbolId, FinalizerSymbolId, FunctionSymbolId, GenericConstParameterSymbolId,
    GenericParameterSymbolId, GenericTypeParameterSymbolId, ImplementationOverloadSymbolId,
    ImplementationSymbolId, InherentImplementationSymbolId, InherentTypeMemberSymbolId,
    ModuleOwnerId, ModuleSymbolId, NamedTraitImplementationSymbolId, NamedTypeSymbolId,
    PackageSymbolId, ParameterSymbolId, PredicateParameterSymbolId, PredicateSymbolId,
    ReceiverParameterSymbolId, ScopeEnterSymbolId, ScopeExitSymbolId,
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
    ImportedIdentitySurfaceError, ImportedPackageIdentitySurface, ImportedSymbolFactKey,
    ImportedSymbolIdentity, ImportedSymbolIdentityInput,
};
pub use interface::{ImportedInterfaceId, InterfaceSupportEntityId, InterfaceSymbolId};
pub use key::{
    ModulePathKey, PackageIdentity, SymbolKey, SymbolKeyData, SymbolOrdinal, SymbolRootKey,
    SynthesizedSymbolKey, SynthesizedSymbolRole,
};
pub use kind::SymbolKind;
pub use origin::SymbolOrigin;
