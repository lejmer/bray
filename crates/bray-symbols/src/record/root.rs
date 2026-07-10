use bray_declarations::{DeclarationId, ModulePartId};

use crate::relationship::ModuleRelationships;
use crate::{
    CompilerKnownEnvironmentSymbolId, ModuleOwnerId, ModulePathKey, ModuleSymbolId,
    PackageIdentity, PackageSymbolId, SymbolKey, SymbolOrigin,
};

/// The single compilation-local root for ambient compiler-known declarations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilerKnownEnvironmentSymbol {
    id: CompilerKnownEnvironmentSymbolId,
    key: SymbolKey,
    modules: Box<[ModuleSymbolId]>,
}

impl CompilerKnownEnvironmentSymbol {
    pub(crate) fn new(
        id: CompilerKnownEnvironmentSymbolId,
        key: SymbolKey,
        modules: Box<[ModuleSymbolId]>,
    ) -> Self {
        Self { id, key, modules }
    }

    /// Returns this root's exact compilation-local ID.
    pub const fn id(&self) -> CompilerKnownEnvironmentSymbolId {
        self.id
    }

    /// Returns this root's deterministic key.
    pub const fn key(&self) -> &SymbolKey {
        &self.key
    }

    /// Returns this root's origin.
    pub const fn origin(&self) -> SymbolOrigin {
        SymbolOrigin::CompilerKnown
    }

    /// Returns compiler-known modules in stable identity order.
    pub fn modules(&self) -> &[ModuleSymbolId] {
        &self.modules
    }
}

/// The immutable identity record for one package root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageSymbol {
    id: PackageSymbolId,
    key: SymbolKey,
    identity: PackageIdentity,
    origin: SymbolOrigin,
    modules: Box<[ModuleSymbolId]>,
}

impl PackageSymbol {
    pub(crate) fn new(
        id: PackageSymbolId,
        key: SymbolKey,
        identity: PackageIdentity,
        origin: SymbolOrigin,
        modules: Box<[ModuleSymbolId]>,
    ) -> Self {
        Self {
            id,
            key,
            identity,
            origin,
            modules,
        }
    }

    /// Returns this package's exact compilation-local ID.
    pub const fn id(&self) -> PackageSymbolId {
        self.id
    }

    /// Returns this package's deterministic key.
    pub const fn key(&self) -> &SymbolKey {
        &self.key
    }

    /// Returns the package-layer identity represented by this symbol.
    pub const fn identity(&self) -> &PackageIdentity {
        &self.identity
    }

    /// Returns this package's origin.
    pub const fn origin(&self) -> SymbolOrigin {
        self.origin
    }

    /// Returns the package's logical modules in stable identity order.
    pub fn modules(&self) -> &[ModuleSymbolId] {
        &self.modules
    }
}

/// The immutable identity record for one logical module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleSymbol {
    id: ModuleSymbolId,
    key: SymbolKey,
    owner: ModuleOwnerId,
    path: ModulePathKey,
    origin: SymbolOrigin,
    declarations: Box<[DeclarationId]>,
    module_parts: Box<[ModulePartId]>,
    is_recovered: bool,
    relationships: ModuleRelationships,
}

impl ModuleSymbol {
    pub(crate) fn new(input: ModuleSymbolInput) -> Self {
        Self {
            id: input.id,
            key: input.key,
            owner: input.owner,
            path: input.path,
            origin: input.origin,
            declarations: input.declarations,
            module_parts: input.module_parts,
            is_recovered: input.is_recovered,
            relationships: ModuleRelationships::default(),
        }
    }

    pub(crate) fn with_relationships(mut self, relationships: ModuleRelationships) -> Self {
        self.relationships = relationships;
        self
    }

    /// Returns this module's exact compilation-local ID.
    pub const fn id(&self) -> ModuleSymbolId {
        self.id
    }

    /// Returns this module's deterministic key.
    pub const fn key(&self) -> &SymbolKey {
        &self.key
    }

    /// Returns the package or compiler-known environment that owns this module.
    pub const fn owner(&self) -> ModuleOwnerId {
        self.owner
    }

    /// Returns this module's full logical path.
    pub const fn path(&self) -> &ModulePathKey {
        &self.path
    }

    /// Returns this module's origin.
    pub const fn origin(&self) -> SymbolOrigin {
        self.origin
    }

    /// Returns every module declaration contributing to this logical module.
    pub fn declarations(&self) -> &[DeclarationId] {
        &self.declarations
    }

    /// Returns every declaration-discovery module part contributing to this module.
    pub fn module_parts(&self) -> &[ModulePartId] {
        &self.module_parts
    }

    /// Returns whether any contributing module declaration contains parser recovery.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }

    /// Returns constants declared by this module in canonical source order.
    pub fn constants(&self) -> &[crate::ConstantSymbolId] {
        &self.relationships.constants
    }

    /// Returns functions declared by this module in canonical source order.
    pub fn functions(&self) -> &[crate::FunctionSymbolId] {
        &self.relationships.functions
    }

    /// Returns predicates declared by this module in canonical source order.
    pub fn predicates(&self) -> &[crate::PredicateSymbolId] {
        &self.relationships.predicates
    }

    /// Returns callable contracts declared by this module in canonical source order.
    pub fn callable_contracts(&self) -> &[crate::CallableContractSymbolId] {
        &self.relationships.callable_contracts
    }

    /// Returns callable overload families in canonical source order.
    pub fn callable_overloads(&self) -> &[crate::CallableOverloadSymbolId] {
        &self.relationships.callable_overloads
    }

    /// Returns implementation overload families in canonical source order.
    pub fn implementation_overloads(&self) -> &[crate::ImplementationOverloadSymbolId] {
        &self.relationships.implementation_overloads
    }

    /// Returns structs declared by this module in canonical source order.
    pub fn structures(&self) -> &[crate::StructSymbolId] {
        &self.relationships.structures
    }

    /// Returns unions declared by this module in canonical source order.
    pub fn unions(&self) -> &[crate::UnionSymbolId] {
        &self.relationships.unions
    }

    /// Returns traits declared by this module in canonical source order.
    pub fn traits(&self) -> &[crate::TraitSymbolId] {
        &self.relationships.traits
    }

    /// Returns inherent implementations in canonical source order.
    pub fn inherent_implementations(&self) -> &[crate::InherentImplementationSymbolId] {
        &self.relationships.inherent_implementations
    }

    /// Returns unnamed trait implementations in canonical source order.
    pub fn unnamed_trait_implementations(&self) -> &[crate::UnnamedTraitImplementationSymbolId] {
        &self.relationships.unnamed_trait_implementations
    }

    /// Returns named trait implementations in canonical source order.
    pub fn named_trait_implementations(&self) -> &[crate::NamedTraitImplementationSymbolId] {
        &self.relationships.named_trait_implementations
    }
}

pub(crate) struct ModuleSymbolInput {
    pub(crate) id: ModuleSymbolId,
    pub(crate) key: SymbolKey,
    pub(crate) owner: ModuleOwnerId,
    pub(crate) path: ModulePathKey,
    pub(crate) origin: SymbolOrigin,
    pub(crate) declarations: Box<[DeclarationId]>,
    pub(crate) module_parts: Box<[ModulePartId]>,
    pub(crate) is_recovered: bool,
}

#[cfg(test)]
mod tests {
    use super::{CompilerKnownEnvironmentSymbol, ModuleSymbol, PackageSymbol};

    #[test]
    fn root_and_module_records_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CompilerKnownEnvironmentSymbol>();
        assert_send_sync::<PackageSymbol>();
        assert_send_sync::<ModuleSymbol>();
    }
}
