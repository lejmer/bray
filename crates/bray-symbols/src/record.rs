use bray_declarations::{DeclarationId, ModulePartId, SyntaxAnchor};

use crate::{
    AnySymbolId, CompilerKnownEnvironmentSymbolId, ModuleOwnerId, ModulePathKey, ModuleSymbolId,
    PackageIdentity, PackageSymbolId, SymbolKey, SymbolOrigin,
};

macro_rules! for_each_source_symbol {
    ($consumer:ident) => {
        $consumer! {
            ConstantSymbol, ConstantSymbolId, Constant, constant, constants;
            FunctionSymbol, FunctionSymbolId, Function, function, functions;
            PredicateSymbol, PredicateSymbolId, Predicate, predicate, predicates;
            CallableContractSymbol, CallableContractSymbolId, CallableContract, callable_contract, callable_contracts;
            CallableOverloadSymbol, CallableOverloadSymbolId, CallableOverload, callable_overload, callable_overloads;
            ImplementationOverloadSymbol, ImplementationOverloadSymbolId, ImplementationOverload, implementation_overload, implementation_overloads;
            StructSymbol, StructSymbolId, Struct, structure, structures;
            UnionSymbol, UnionSymbolId, Union, union, unions;
            TraitSymbol, TraitSymbolId, Trait, trait_symbol, traits;
            InherentImplementationSymbol, InherentImplementationSymbolId, InherentImplementation, inherent_implementation, inherent_implementations;
            UnnamedTraitImplementationSymbol, UnnamedTraitImplementationSymbolId, UnnamedTraitImplementation, unnamed_trait_implementation, unnamed_trait_implementations;
            NamedTraitImplementationSymbol, NamedTraitImplementationSymbolId, NamedTraitImplementation, named_trait_implementation, named_trait_implementations;
            StructFieldSymbol, StructFieldSymbolId, StructField, struct_field, struct_fields;
            UnionVariantSymbol, UnionVariantSymbolId, UnionVariant, union_variant, union_variants;
            UnionPayloadFieldSymbol, UnionPayloadFieldSymbolId, UnionPayloadField, union_payload_field, union_payload_fields;
            TypeCallableMemberSymbol, TypeCallableMemberSymbolId, TypeCallableMember, type_callable_member, type_callable_members;
            ConstructorSymbol, ConstructorSymbolId, Constructor, constructor, constructors;
            FinalizerSymbol, FinalizerSymbolId, Finalizer, finalizer, finalizers;
            DestructorSymbol, DestructorSymbolId, Destructor, destructor, destructors;
            ScopeEnterSymbol, ScopeEnterSymbolId, ScopeEnter, scope_enter, scope_enters;
            ScopeExitSymbol, ScopeExitSymbolId, ScopeExit, scope_exit, scope_exits;
            InherentTypeMemberSymbol, InherentTypeMemberSymbolId, InherentTypeMember, inherent_type_member, inherent_type_members;
            TraitCallableMemberSymbol, TraitCallableMemberSymbolId, TraitCallableMember, trait_callable_member, trait_callable_members;
            TraitConstantMemberSymbol, TraitConstantMemberSymbolId, TraitConstantMember, trait_constant_member, trait_constant_members;
            TraitTypeMemberSymbol, TraitTypeMemberSymbolId, TraitTypeMember, trait_type_member, trait_type_members;
            TraitPredicateMemberSymbol, TraitPredicateMemberSymbolId, TraitPredicateMember, trait_predicate_member, trait_predicate_members;
            TraitFinalizerRequirementSymbol, TraitFinalizerRequirementSymbolId, TraitFinalizerRequirement, trait_finalizer_requirement, trait_finalizer_requirements;
            TraitDestructorRequirementSymbol, TraitDestructorRequirementSymbolId, TraitDestructorRequirement, trait_destructor_requirement, trait_destructor_requirements;
            TraitScopeEnterRequirementSymbol, TraitScopeEnterRequirementSymbolId, TraitScopeEnterRequirement, trait_scope_enter_requirement, trait_scope_enter_requirements;
            TraitScopeExitRequirementSymbol, TraitScopeExitRequirementSymbolId, TraitScopeExitRequirement, trait_scope_exit_requirement, trait_scope_exit_requirements;
            TraitCallableFulfillmentSymbol, TraitCallableFulfillmentSymbolId, TraitCallableFulfillment, trait_callable_fulfillment, trait_callable_fulfillments;
            TraitConstantFulfillmentSymbol, TraitConstantFulfillmentSymbolId, TraitConstantFulfillment, trait_constant_fulfillment, trait_constant_fulfillments;
            TraitTypeFulfillmentSymbol, TraitTypeFulfillmentSymbolId, TraitTypeFulfillment, trait_type_fulfillment, trait_type_fulfillments;
            TraitPredicateFulfillmentSymbol, TraitPredicateFulfillmentSymbolId, TraitPredicateFulfillment, trait_predicate_fulfillment, trait_predicate_fulfillments;
            TraitScopeEnterFulfillmentSymbol, TraitScopeEnterFulfillmentSymbolId, TraitScopeEnterFulfillment, trait_scope_enter_fulfillment, trait_scope_enter_fulfillments;
            TraitScopeExitFulfillmentSymbol, TraitScopeExitFulfillmentSymbolId, TraitScopeExitFulfillment, trait_scope_exit_fulfillment, trait_scope_exit_fulfillments;
            GenericTypeParameterSymbol, GenericTypeParameterSymbolId, GenericTypeParameter, generic_type_parameter, generic_type_parameters;
            GenericConstParameterSymbol, GenericConstParameterSymbolId, GenericConstParameter, generic_const_parameter, generic_const_parameters;
            CallableParameterSymbol, CallableParameterSymbolId, CallableParameter, callable_parameter, callable_parameters;
            PredicateParameterSymbol, PredicateParameterSymbolId, PredicateParameter, predicate_parameter, predicate_parameters;
        }
    };
}

pub(crate) use for_each_source_symbol;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceSymbolIdentity {
    key: SymbolKey,
    containing_symbol: AnySymbolId,
    declaration: DeclarationId,
    syntax: SyntaxAnchor,
}

impl SourceSymbolIdentity {
    pub(crate) const fn new(
        key: SymbolKey,
        containing_symbol: AnySymbolId,
        declaration: DeclarationId,
        syntax: SyntaxAnchor,
    ) -> Self {
        Self {
            key,
            containing_symbol,
            declaration,
            syntax,
        }
    }
}

macro_rules! define_source_symbol_records {
    ($($record:ident, $id:ident, $variant:ident, $singular:ident, $plural:ident;)+) => {
        $(
            #[doc = concat!("The immutable identity record for a source `", stringify!($variant), "` symbol.")]
            #[derive(Clone, Debug, Eq, PartialEq)]
            pub struct $record {
                id: crate::$id,
                identity: SourceSymbolIdentity,
            }

            impl $record {
                pub(crate) const fn new(id: crate::$id, identity: SourceSymbolIdentity) -> Self {
                    Self { id, identity }
                }

                /// Returns this symbol's exact compilation-local ID.
                pub const fn id(&self) -> crate::$id {
                    self.id
                }

                /// Returns this symbol's deterministic construction key.
                pub const fn key(&self) -> &SymbolKey {
                    &self.identity.key
                }

                /// Returns this symbol's origin.
                pub const fn origin(&self) -> SymbolOrigin {
                    SymbolOrigin::Source
                }

                /// Returns this symbol's immediate semantic container.
                pub const fn containing_symbol(&self) -> AnySymbolId {
                    self.identity.containing_symbol
                }

                /// Returns the declaration that introduced this symbol.
                pub const fn declaration(&self) -> DeclarationId {
                    self.identity.declaration
                }

                /// Returns the stable syntax anchor that introduced this symbol.
                pub const fn syntax_anchor(&self) -> SyntaxAnchor {
                    self.identity.syntax
                }

                /// Returns whether the introducing syntax contains parser recovery.
                pub const fn is_recovered(&self) -> bool {
                    self.identity.syntax.is_recovered()
                }
            }
        )+
    };
}

for_each_source_symbol!(define_source_symbol_records);

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
        }
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
