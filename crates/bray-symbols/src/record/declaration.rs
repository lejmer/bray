use bray_compiler_known::{CatalogDeclarationSurface, CompilerKnownDeclarationId};
use bray_declarations::{DeclarationId, SyntaxAnchor};

use crate::relationship::{
    BuildRelationships, CallableParameterRelationships, CallableRelationships,
    GenericParameterRelationships, GenericRelationships, ImplementationRelationships,
    LeafRelationships, OverloadRelationships, PredicateParameterRelationships,
    PredicateRelationships, RelationshipIndex, RuntimeDefaultPresence, StructFieldRelationships,
    StructRelationships, TraitRelationships, UnionPayloadFieldRelationships, UnionRelationships,
    VariantRelationships,
};
use crate::{
    AnySymbolId, ImportedInterfaceId, ImportedSymbolFactKey, InterfaceSymbolId, SymbolKey,
    SymbolName, SymbolOrigin,
};

macro_rules! for_each_declaration_symbol {
    ($consumer:ident) => {
        $consumer! {
            ConstantSymbol, ConstantSymbolId, Constant, constant, constants, LeafRelationships;
            FunctionSymbol, FunctionSymbolId, Function, function, functions, CallableRelationships;
            PredicateSymbol, PredicateSymbolId, Predicate, predicate, predicates, PredicateRelationships;
            CallableContractSymbol, CallableContractSymbolId, CallableContract, callable_contract, callable_contracts, GenericRelationships;
            CallableOverloadSymbol, CallableOverloadSymbolId, CallableOverload, callable_overload, callable_overloads, OverloadRelationships;
            ImplementationOverloadSymbol, ImplementationOverloadSymbolId, ImplementationOverload, implementation_overload, implementation_overloads, OverloadRelationships;
            StructSymbol, StructSymbolId, Struct, structure, structures, StructRelationships;
            UnionSymbol, UnionSymbolId, Union, union, unions, UnionRelationships;
            TraitSymbol, TraitSymbolId, Trait, trait_symbol, traits, TraitRelationships;
            InherentImplementationSymbol, InherentImplementationSymbolId, InherentImplementation, inherent_implementation, inherent_implementations, ImplementationRelationships;
            UnnamedTraitImplementationSymbol, UnnamedTraitImplementationSymbolId, UnnamedTraitImplementation, unnamed_trait_implementation, unnamed_trait_implementations, ImplementationRelationships;
            NamedTraitImplementationSymbol, NamedTraitImplementationSymbolId, NamedTraitImplementation, named_trait_implementation, named_trait_implementations, ImplementationRelationships;
            StructFieldSymbol, StructFieldSymbolId, StructField, struct_field, struct_fields, StructFieldRelationships;
            UnionVariantSymbol, UnionVariantSymbolId, UnionVariant, union_variant, union_variants, VariantRelationships;
            UnionPayloadFieldSymbol, UnionPayloadFieldSymbolId, UnionPayloadField, union_payload_field, union_payload_fields, UnionPayloadFieldRelationships;
            TypeCallableMemberSymbol, TypeCallableMemberSymbolId, TypeCallableMember, type_callable_member, type_callable_members, CallableRelationships;
            ConstructorSymbol, ConstructorSymbolId, Constructor, constructor, constructors, CallableRelationships;
            FinalizerSymbol, FinalizerSymbolId, Finalizer, finalizer, finalizers, CallableRelationships;
            DestructorSymbol, DestructorSymbolId, Destructor, destructor, destructors, CallableRelationships;
            ScopeEnterSymbol, ScopeEnterSymbolId, ScopeEnter, scope_enter, scope_enters, CallableRelationships;
            ScopeExitSymbol, ScopeExitSymbolId, ScopeExit, scope_exit, scope_exits, CallableRelationships;
            InherentTypeMemberSymbol, InherentTypeMemberSymbolId, InherentTypeMember, inherent_type_member, inherent_type_members, LeafRelationships;
            TraitCallableMemberSymbol, TraitCallableMemberSymbolId, TraitCallableMember, trait_callable_member, trait_callable_members, CallableRelationships;
            TraitConstantMemberSymbol, TraitConstantMemberSymbolId, TraitConstantMember, trait_constant_member, trait_constant_members, LeafRelationships;
            TraitTypeMemberSymbol, TraitTypeMemberSymbolId, TraitTypeMember, trait_type_member, trait_type_members, LeafRelationships;
            TraitPredicateMemberSymbol, TraitPredicateMemberSymbolId, TraitPredicateMember, trait_predicate_member, trait_predicate_members, PredicateRelationships;
            TraitFinalizerRequirementSymbol, TraitFinalizerRequirementSymbolId, TraitFinalizerRequirement, trait_finalizer_requirement, trait_finalizer_requirements, CallableRelationships;
            TraitDestructorRequirementSymbol, TraitDestructorRequirementSymbolId, TraitDestructorRequirement, trait_destructor_requirement, trait_destructor_requirements, CallableRelationships;
            TraitScopeEnterRequirementSymbol, TraitScopeEnterRequirementSymbolId, TraitScopeEnterRequirement, trait_scope_enter_requirement, trait_scope_enter_requirements, CallableRelationships;
            TraitScopeExitRequirementSymbol, TraitScopeExitRequirementSymbolId, TraitScopeExitRequirement, trait_scope_exit_requirement, trait_scope_exit_requirements, CallableRelationships;
            TraitCallableFulfillmentSymbol, TraitCallableFulfillmentSymbolId, TraitCallableFulfillment, trait_callable_fulfillment, trait_callable_fulfillments, CallableRelationships;
            TraitConstantFulfillmentSymbol, TraitConstantFulfillmentSymbolId, TraitConstantFulfillment, trait_constant_fulfillment, trait_constant_fulfillments, LeafRelationships;
            TraitTypeFulfillmentSymbol, TraitTypeFulfillmentSymbolId, TraitTypeFulfillment, trait_type_fulfillment, trait_type_fulfillments, LeafRelationships;
            TraitPredicateFulfillmentSymbol, TraitPredicateFulfillmentSymbolId, TraitPredicateFulfillment, trait_predicate_fulfillment, trait_predicate_fulfillments, PredicateRelationships;
            TraitScopeEnterFulfillmentSymbol, TraitScopeEnterFulfillmentSymbolId, TraitScopeEnterFulfillment, trait_scope_enter_fulfillment, trait_scope_enter_fulfillments, CallableRelationships;
            TraitScopeExitFulfillmentSymbol, TraitScopeExitFulfillmentSymbolId, TraitScopeExitFulfillment, trait_scope_exit_fulfillment, trait_scope_exit_fulfillments, CallableRelationships;
            GenericTypeParameterSymbol, GenericTypeParameterSymbolId, GenericTypeParameter, generic_type_parameter, generic_type_parameters, GenericParameterRelationships;
            GenericConstParameterSymbol, GenericConstParameterSymbolId, GenericConstParameter, generic_const_parameter, generic_const_parameters, GenericParameterRelationships;
            CallableParameterSymbol, CallableParameterSymbolId, CallableParameter, callable_parameter, callable_parameters, CallableParameterRelationships;
            PredicateParameterSymbol, PredicateParameterSymbolId, PredicateParameter, predicate_parameter, predicate_parameters, PredicateParameterRelationships;
        }
    };
}

pub(crate) use for_each_declaration_symbol;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DeclarationSymbolIdentity {
    Source {
        key: SymbolKey,
        containing_symbol: AnySymbolId,
        declaration: DeclarationId,
        syntax: SyntaxAnchor,
    },
    InferredImplementationParameter {
        key: SymbolKey,
        containing_symbol: AnySymbolId,
        name: SymbolName,
        inference_sources: Box<[SyntaxAnchor]>,
        is_recovered: bool,
    },
    CompilerKnown {
        key: SymbolKey,
        containing_symbol: AnySymbolId,
        declaration: CompilerKnownDeclarationId,
        surface: CatalogDeclarationSurface,
        origin: SymbolOrigin,
        parameter_name: Option<SymbolName>,
    },
    Imported {
        key: SymbolKey,
        containing_symbol: AnySymbolId,
        backing: ImportedSymbolBacking,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ImportedSymbolBacking {
    interface: ImportedInterfaceId,
    symbol: InterfaceSymbolId,
}

impl ImportedSymbolBacking {
    pub(crate) const fn new(interface: ImportedInterfaceId, symbol: InterfaceSymbolId) -> Self {
        Self { interface, symbol }
    }

    pub(crate) fn fact_key<I: crate::ExactSymbolId>(self) -> ImportedSymbolFactKey<I> {
        ImportedSymbolFactKey::from_validated(self.interface, self.symbol)
    }
}

impl DeclarationSymbolIdentity {
    pub(crate) const fn new(
        key: SymbolKey,
        containing_symbol: AnySymbolId,
        declaration: DeclarationId,
        syntax: SyntaxAnchor,
    ) -> Self {
        Self::Source {
            key,
            containing_symbol,
            declaration,
            syntax,
        }
    }

    pub(crate) const fn compiler_known(
        key: SymbolKey,
        containing_symbol: AnySymbolId,
        declaration: CompilerKnownDeclarationId,
        surface: CatalogDeclarationSurface,
        origin: SymbolOrigin,
    ) -> Self {
        Self::CompilerKnown {
            key,
            containing_symbol,
            declaration,
            surface,
            origin,
            parameter_name: None,
        }
    }

    pub(crate) const fn compiler_known_parameter(
        key: SymbolKey,
        containing_symbol: AnySymbolId,
        declaration: CompilerKnownDeclarationId,
        surface: CatalogDeclarationSurface,
        origin: SymbolOrigin,
        name: SymbolName,
    ) -> Self {
        Self::CompilerKnown {
            key,
            containing_symbol,
            declaration,
            surface,
            origin,
            parameter_name: Some(name),
        }
    }

    pub(crate) fn inferred_implementation_parameter(
        key: SymbolKey,
        containing_symbol: AnySymbolId,
        name: SymbolName,
        inference_sources: impl IntoIterator<Item = SyntaxAnchor>,
    ) -> Self {
        let inference_sources = inference_sources.into_iter().collect::<Box<[_]>>();
        let is_recovered = inference_sources.iter().any(|source| source.is_recovered());

        Self::InferredImplementationParameter {
            key,
            containing_symbol,
            name,
            inference_sources,
            is_recovered,
        }
    }

    pub(crate) const fn imported(
        key: SymbolKey,
        containing_symbol: AnySymbolId,
        backing: ImportedSymbolBacking,
    ) -> Self {
        Self::Imported {
            key,
            containing_symbol,
            backing,
        }
    }

    pub(crate) const fn containing_symbol(&self) -> AnySymbolId {
        match self {
            Self::Source {
                containing_symbol, ..
            }
            | Self::InferredImplementationParameter {
                containing_symbol, ..
            }
            | Self::CompilerKnown {
                containing_symbol, ..
            }
            | Self::Imported {
                containing_symbol, ..
            } => *containing_symbol,
        }
    }

    pub(crate) const fn source_declaration(&self) -> Option<DeclarationId> {
        match self {
            Self::Source { declaration, .. } => Some(*declaration),
            Self::InferredImplementationParameter { .. }
            | Self::CompilerKnown { .. }
            | Self::Imported { .. } => None,
        }
    }

    const fn key(&self) -> &SymbolKey {
        match self {
            Self::Source { key, .. }
            | Self::InferredImplementationParameter { key, .. }
            | Self::CompilerKnown { key, .. }
            | Self::Imported { key, .. } => key,
        }
    }

    const fn origin(&self) -> SymbolOrigin {
        match self {
            Self::Source { .. } => SymbolOrigin::Source,
            Self::InferredImplementationParameter { .. } => SymbolOrigin::Synthesized,
            Self::CompilerKnown { origin, .. } => *origin,
            Self::Imported { .. } => SymbolOrigin::Imported,
        }
    }

    const fn syntax_anchor(&self) -> Option<SyntaxAnchor> {
        match self {
            Self::Source { syntax, .. } => Some(*syntax),
            Self::InferredImplementationParameter {
                inference_sources, ..
            } => inference_sources.first().copied(),
            Self::CompilerKnown { .. } | Self::Imported { .. } => None,
        }
    }

    fn imported_fact_key<I: crate::ExactSymbolId>(&self) -> Option<ImportedSymbolFactKey<I>> {
        match self {
            Self::Imported { backing, .. } => Some(backing.fact_key()),
            Self::Source { .. }
            | Self::InferredImplementationParameter { .. }
            | Self::CompilerKnown { .. } => None,
        }
    }

    const fn inferred_name(&self) -> Option<&SymbolName> {
        match self {
            Self::InferredImplementationParameter { name, .. } => Some(name),
            Self::CompilerKnown { parameter_name, .. } => parameter_name.as_ref(),
            Self::Source { .. } | Self::Imported { .. } => None,
        }
    }

    fn inference_sources(&self) -> &[SyntaxAnchor] {
        match self {
            Self::InferredImplementationParameter {
                inference_sources, ..
            } => inference_sources,
            Self::Source { .. } | Self::CompilerKnown { .. } | Self::Imported { .. } => &[],
        }
    }
}

macro_rules! define_declaration_symbol_records {
    ($($record:ident, $id:ident, $variant:ident, $singular:ident, $plural:ident, $relationships:ty;)+) => {
        $(
            #[doc = concat!("The immutable identity record for a `", stringify!($variant), "` symbol.")]
            #[derive(Clone, Debug, Eq, PartialEq)]
            pub struct $record {
                id: crate::$id,
                identity: DeclarationSymbolIdentity,
                relationships: $relationships,
            }

            impl $record {
                pub(crate) fn new(
                    id: crate::$id,
                    identity: DeclarationSymbolIdentity,
                    relationship_index: &RelationshipIndex,
                ) -> Option<Self> {
                    let relationships =
                        <$relationships as BuildRelationships<crate::$id>>::build(
                            id,
                            relationship_index,
                        )?;

                    Some(Self {
                        id,
                        identity,
                        relationships,
                    })
                }

                /// Returns this symbol's exact compilation-local ID.
                pub const fn id(&self) -> crate::$id {
                    self.id
                }

                /// Returns this symbol's stable semantic key.
                pub const fn key(&self) -> &SymbolKey {
                    self.identity.key()
                }

                /// Returns this symbol's origin.
                pub const fn origin(&self) -> SymbolOrigin {
                    self.identity.origin()
                }

                /// Returns this symbol's immediate semantic container.
                pub const fn containing_symbol(&self) -> AnySymbolId {
                    self.identity.containing_symbol()
                }

                /// Returns the declaration that introduced this symbol.
                pub const fn declaration(&self) -> Option<DeclarationId> {
                    self.identity.source_declaration()
                }

                /// Returns the stable syntax anchor that introduced this symbol.
                pub const fn syntax_anchor(&self) -> Option<SyntaxAnchor> {
                    self.identity.syntax_anchor()
                }

                /// Returns the catalog-local declaration identity for compiler-known symbols.
                pub const fn compiler_known_declaration(
                    &self,
                ) -> Option<CompilerKnownDeclarationId> {
                    match &self.identity {
                        DeclarationSymbolIdentity::Source { .. }
                        | DeclarationSymbolIdentity::InferredImplementationParameter { .. } => None,
                        DeclarationSymbolIdentity::Imported { .. } => None,
                        DeclarationSymbolIdentity::CompilerKnown { declaration, .. } => {
                            Some(*declaration)
                        }
                    }
                }

                /// Returns the generated declaration surface for compiler-known symbols.
                pub const fn compiler_known_surface(&self) -> Option<CatalogDeclarationSurface> {
                    match &self.identity {
                        DeclarationSymbolIdentity::Source { .. }
                        | DeclarationSymbolIdentity::InferredImplementationParameter { .. } => None,
                        DeclarationSymbolIdentity::Imported { .. } => None,
                        DeclarationSymbolIdentity::CompilerKnown { surface, .. } => Some(*surface),
                    }
                }

                /// Returns the imported semantic-fact key, when available.
                pub fn imported_fact_key(&self) -> Option<ImportedSymbolFactKey<crate::$id>> {
                    self.identity.imported_fact_key()
                }

                /// Returns whether the introducing syntax contains parser recovery.
                pub const fn is_recovered(&self) -> bool {
                    match &self.identity {
                        DeclarationSymbolIdentity::Source { syntax, .. } => syntax.is_recovered(),
                        DeclarationSymbolIdentity::InferredImplementationParameter {
                            is_recovered, ..
                        } => *is_recovered,
                        DeclarationSymbolIdentity::CompilerKnown { .. }
                        | DeclarationSymbolIdentity::Imported { .. } => false,
                    }
                }
            }
        )+
    };
}

for_each_declaration_symbol!(define_declaration_symbol_records);

macro_rules! impl_generic_relationships {
    ($($record:ident),+ $(,)?) => {
        $(
            impl $record {
                /// Returns generic type parameters in declaration order.
                pub fn generic_type_parameters(&self) -> &[crate::GenericTypeParameterSymbolId] {
                    &self.relationships.type_parameters
                }

                /// Returns generic constant parameters in declaration order.
                pub fn generic_const_parameters(&self) -> &[crate::GenericConstParameterSymbolId] {
                    &self.relationships.const_parameters
                }
            }
        )+
    };
}

macro_rules! impl_callable_relationships {
    ($($record:ident),+ $(,)?) => {
        $(
            impl $record {
                /// Returns generic type parameters in declaration order.
                pub fn generic_type_parameters(&self) -> &[crate::GenericTypeParameterSymbolId] {
                    &self.relationships.generics.type_parameters
                }

                /// Returns generic constant parameters in declaration order.
                pub fn generic_const_parameters(&self) -> &[crate::GenericConstParameterSymbolId] {
                    &self.relationships.generics.const_parameters
                }

                /// Returns written callable parameters in declaration order.
                pub fn parameters(&self) -> &[crate::CallableParameterSymbolId] {
                    &self.relationships.parameters
                }

                /// Returns the implicit receiver when this callable is instance-associated.
                pub const fn receiver(&self) -> Option<crate::ReceiverParameterSymbolId> {
                    self.relationships.receiver
                }
            }
        )+
    };
}

macro_rules! impl_predicate_relationships {
    ($($record:ident),+ $(,)?) => {
        $(
            impl $record {
                /// Returns generic type parameters in declaration order.
                pub fn generic_type_parameters(&self) -> &[crate::GenericTypeParameterSymbolId] {
                    &self.relationships.generics.type_parameters
                }

                /// Returns generic constant parameters in declaration order.
                pub fn generic_const_parameters(&self) -> &[crate::GenericConstParameterSymbolId] {
                    &self.relationships.generics.const_parameters
                }

                /// Returns predicate parameters in declaration order.
                pub fn parameters(&self) -> &[crate::PredicateParameterSymbolId] {
                    &self.relationships.parameters
                }
            }
        )+
    };
}

impl_generic_relationships!(CallableContractSymbol);

impl_callable_relationships!(
    FunctionSymbol,
    TypeCallableMemberSymbol,
    ConstructorSymbol,
    FinalizerSymbol,
    DestructorSymbol,
    ScopeEnterSymbol,
    ScopeExitSymbol,
    TraitCallableMemberSymbol,
    TraitFinalizerRequirementSymbol,
    TraitDestructorRequirementSymbol,
    TraitScopeEnterRequirementSymbol,
    TraitScopeExitRequirementSymbol,
    TraitCallableFulfillmentSymbol,
    TraitScopeEnterFulfillmentSymbol,
    TraitScopeExitFulfillmentSymbol,
);

impl_predicate_relationships!(
    PredicateSymbol,
    TraitPredicateMemberSymbol,
    TraitPredicateFulfillmentSymbol,
);

impl CallableOverloadSymbol {
    /// Returns unresolved overload-arm path anchors in source order.
    pub fn arm_syntax(&self) -> &[SyntaxAnchor] {
        &self.relationships.arm_syntax
    }

    /// Returns imported overload-arm symbols in interface order.
    pub fn arms(&self) -> &[AnySymbolId] {
        &self.relationships.arms
    }
}

impl ImplementationOverloadSymbol {
    /// Returns unresolved overload-arm path anchors in source order.
    pub fn arm_syntax(&self) -> &[SyntaxAnchor] {
        &self.relationships.arm_syntax
    }

    /// Returns imported overload-arm symbols in interface order.
    pub fn arms(&self) -> &[AnySymbolId] {
        &self.relationships.arms
    }
}

macro_rules! impl_named_type_relationships {
    ($record:ident) => {
        impl $record {
            /// Returns generic type parameters in declaration order.
            pub fn generic_type_parameters(&self) -> &[crate::GenericTypeParameterSymbolId] {
                &self.relationships.generics.type_parameters
            }

            /// Returns generic constant parameters in declaration order.
            pub fn generic_const_parameters(&self) -> &[crate::GenericConstParameterSymbolId] {
                &self.relationships.generics.const_parameters
            }

            /// Returns directly declared constructors in source order.
            pub fn constructors(&self) -> &[crate::ConstructorSymbolId] {
                &self.relationships.constructors
            }

            /// Returns directly declared finalizers in source order.
            pub fn finalizers(&self) -> &[crate::FinalizerSymbolId] {
                &self.relationships.finalizers
            }

            /// Returns directly declared destructors in source order.
            pub fn destructors(&self) -> &[crate::DestructorSymbolId] {
                &self.relationships.destructors
            }

            /// Returns directly declared scope-enter members in source order.
            pub fn scope_enters(&self) -> &[crate::ScopeEnterSymbolId] {
                &self.relationships.scope_enters
            }

            /// Returns directly declared scope-exit members in source order.
            pub fn scope_exits(&self) -> &[crate::ScopeExitSymbolId] {
                &self.relationships.scope_exits
            }

            /// Returns directly declared callable members in source order.
            pub fn callable_members(&self) -> &[crate::TypeCallableMemberSymbolId] {
                &self.relationships.callables
            }

            /// Returns directly declared associated constants in source order.
            pub fn constants(&self) -> &[crate::ConstantSymbolId] {
                &self.relationships.constants
            }

            /// Returns directly declared associated predicates in source order.
            pub fn predicates(&self) -> &[crate::PredicateSymbolId] {
                &self.relationships.predicates
            }

            /// Returns directly declared type-valued members in source order.
            pub fn type_members(&self) -> &[crate::InherentTypeMemberSymbolId] {
                &self.relationships.type_members
            }

            /// Returns directly declared callable overload families in source order.
            pub fn callable_overloads(&self) -> &[crate::CallableOverloadSymbolId] {
                &self.relationships.overloads
            }
        }
    };
}

impl_named_type_relationships!(StructSymbol);
impl_named_type_relationships!(UnionSymbol);

impl StructSymbol {
    /// Returns fields in declaration order.
    pub fn fields(&self) -> &[crate::StructFieldSymbolId] {
        &self.relationships.fields
    }
}

impl UnionSymbol {
    /// Returns variants in declaration order.
    pub fn variants(&self) -> &[crate::UnionVariantSymbolId] {
        &self.relationships.variants
    }
}

impl TraitSymbol {
    /// Returns generic type parameters in declaration order.
    pub fn generic_type_parameters(&self) -> &[crate::GenericTypeParameterSymbolId] {
        &self.relationships.generics.type_parameters
    }

    /// Returns generic constant parameters in declaration order.
    pub fn generic_const_parameters(&self) -> &[crate::GenericConstParameterSymbolId] {
        &self.relationships.generics.const_parameters
    }

    /// Returns callable members in declaration order.
    pub fn callable_members(&self) -> &[crate::TraitCallableMemberSymbolId] {
        &self.relationships.callables
    }

    /// Returns constant-valued members in declaration order.
    pub fn constant_members(&self) -> &[crate::TraitConstantMemberSymbolId] {
        &self.relationships.constants
    }

    /// Returns type-valued members in declaration order.
    pub fn type_members(&self) -> &[crate::TraitTypeMemberSymbolId] {
        &self.relationships.types
    }

    /// Returns predicate members in declaration order.
    pub fn predicate_members(&self) -> &[crate::TraitPredicateMemberSymbolId] {
        &self.relationships.predicates
    }

    /// Returns finalizer requirements in declaration order.
    pub fn finalizer_requirements(&self) -> &[crate::TraitFinalizerRequirementSymbolId] {
        &self.relationships.finalizers
    }

    /// Returns destructor requirements in declaration order.
    pub fn destructor_requirements(&self) -> &[crate::TraitDestructorRequirementSymbolId] {
        &self.relationships.destructors
    }

    /// Returns scope-enter requirements in declaration order.
    pub fn scope_enter_requirements(&self) -> &[crate::TraitScopeEnterRequirementSymbolId] {
        &self.relationships.scope_enters
    }

    /// Returns scope-exit requirements in declaration order.
    pub fn scope_exit_requirements(&self) -> &[crate::TraitScopeExitRequirementSymbolId] {
        &self.relationships.scope_exits
    }
}

macro_rules! impl_implementation_relationships {
    ($($record:ident),+ $(,)?) => {
        $(
            impl $record {
                /// Returns generic type parameters in declaration order.
                pub fn generic_type_parameters(&self) -> &[crate::GenericTypeParameterSymbolId] {
                    &self.relationships.generics.type_parameters
                }

                /// Returns generic constant parameters in declaration order.
                pub fn generic_const_parameters(&self) -> &[crate::GenericConstParameterSymbolId] {
                    &self.relationships.generics.const_parameters
                }

                /// Returns inherent callable members in declaration order.
                pub fn callable_members(&self) -> &[crate::TypeCallableMemberSymbolId] {
                    &self.relationships.callables
                }

                /// Returns trait callable fulfillments in declaration order.
                pub fn callable_fulfillments(&self) -> &[crate::TraitCallableFulfillmentSymbolId] {
                    &self.relationships.trait_callables
                }

                /// Returns inherent constants in declaration order.
                pub fn constants(&self) -> &[crate::ConstantSymbolId] {
                    &self.relationships.constants
                }

                /// Returns trait constant fulfillments in declaration order.
                pub fn constant_fulfillments(&self) -> &[crate::TraitConstantFulfillmentSymbolId] {
                    &self.relationships.trait_constants
                }

                /// Returns inherent predicates in declaration order.
                pub fn predicates(&self) -> &[crate::PredicateSymbolId] {
                    &self.relationships.predicates
                }

                /// Returns trait predicate fulfillments in declaration order.
                pub fn predicate_fulfillments(&self) -> &[crate::TraitPredicateFulfillmentSymbolId] {
                    &self.relationships.trait_predicates
                }

                /// Returns inherent type-valued members in declaration order.
                pub fn type_members(&self) -> &[crate::InherentTypeMemberSymbolId] {
                    &self.relationships.types
                }

                /// Returns constructors in declaration order.
                pub fn constructors(&self) -> &[crate::ConstructorSymbolId] {
                    &self.relationships.constructors
                }

                /// Returns finalizers in declaration order.
                pub fn finalizers(&self) -> &[crate::FinalizerSymbolId] {
                    &self.relationships.finalizers
                }

                /// Returns destructors in declaration order.
                pub fn destructors(&self) -> &[crate::DestructorSymbolId] {
                    &self.relationships.destructors
                }

                /// Returns scope-enter members in declaration order.
                pub fn scope_enters(&self) -> &[crate::ScopeEnterSymbolId] {
                    &self.relationships.scope_enters
                }

                /// Returns scope-exit members in declaration order.
                pub fn scope_exits(&self) -> &[crate::ScopeExitSymbolId] {
                    &self.relationships.scope_exits
                }

                /// Returns trait type fulfillments in declaration order.
                pub fn type_fulfillments(&self) -> &[crate::TraitTypeFulfillmentSymbolId] {
                    &self.relationships.trait_types
                }

                /// Returns trait scope-enter fulfillments in declaration order.
                pub fn scope_enter_fulfillments(
                    &self,
                ) -> &[crate::TraitScopeEnterFulfillmentSymbolId] {
                    &self.relationships.trait_scope_enters
                }

                /// Returns trait scope-exit fulfillments in declaration order.
                pub fn scope_exit_fulfillments(
                    &self,
                ) -> &[crate::TraitScopeExitFulfillmentSymbolId] {
                    &self.relationships.trait_scope_exits
                }

                /// Returns callable overload families in declaration order.
                pub fn callable_overloads(&self) -> &[crate::CallableOverloadSymbolId] {
                    &self.relationships.overloads
                }
            }
        )+
    };
}

impl_implementation_relationships!(
    InherentImplementationSymbol,
    UnnamedTraitImplementationSymbol,
    NamedTraitImplementationSymbol,
);

impl UnionVariantSymbol {
    /// Returns the union that owns this variant.
    pub const fn union(&self) -> crate::UnionSymbolId {
        self.relationships.owner
    }

    /// Returns payload fields in declaration order.
    pub fn payload_fields(&self) -> &[crate::UnionPayloadFieldSymbolId] {
        &self.relationships.payload_fields
    }
}

impl GenericTypeParameterSymbol {
    /// Returns the inferred name when this is an implementation parameter.
    pub const fn inferred_name(&self) -> Option<&SymbolName> {
        self.identity.inferred_name()
    }

    /// Returns syntax occurrences that inferred this implementation parameter.
    pub fn inference_sources(&self) -> &[SyntaxAnchor] {
        self.identity.inference_sources()
    }

    /// Returns the declaration that owns this parameter.
    pub const fn owner(&self) -> crate::GenericOwnerId {
        self.relationships.owner
    }

    /// Returns this parameter's source-order ordinal among all generic parameters.
    pub const fn ordinal(&self) -> u32 {
        self.relationships.ordinal
    }
}

impl GenericConstParameterSymbol {
    /// Returns the inferred name when this is an implementation parameter.
    pub const fn inferred_name(&self) -> Option<&SymbolName> {
        self.identity.inferred_name()
    }

    /// Returns syntax occurrences that inferred this implementation parameter.
    pub fn inference_sources(&self) -> &[SyntaxAnchor] {
        self.identity.inference_sources()
    }

    /// Returns the declaration that owns this parameter.
    pub const fn owner(&self) -> crate::GenericOwnerId {
        self.relationships.owner
    }

    /// Returns this parameter's source-order ordinal among all generic parameters.
    pub const fn ordinal(&self) -> u32 {
        self.relationships.ordinal
    }
}

impl CallableParameterSymbol {
    /// Returns the callable that owns this parameter.
    pub const fn owner(&self) -> crate::CallableSymbolId {
        self.relationships.owner
    }

    /// Returns this parameter's source-order ordinal.
    pub const fn ordinal(&self) -> u32 {
        self.relationships.ordinal
    }

    /// Returns the cheap syntax-level default state.
    pub const fn default_presence(&self) -> RuntimeDefaultPresence {
        self.relationships.default_presence
    }

    /// Returns the synthesized provider identity when default syntax exists.
    pub const fn default_provider(
        &self,
    ) -> Option<crate::CallableParameterDefaultProviderSymbolId> {
        self.relationships.default_provider
    }
}

impl PredicateParameterSymbol {
    /// Returns the predicate declaration that owns this parameter.
    pub const fn owner(&self) -> crate::PredicateDefinitionSymbolId {
        self.relationships.owner
    }

    /// Returns this parameter's source-order ordinal.
    pub const fn ordinal(&self) -> u32 {
        self.relationships.ordinal
    }
}

macro_rules! impl_defaultable_field {
    ($record:ident, $owner_method:ident, $owner:ty, $provider:ty) => {
        impl $record {
            /// Returns this field's exact semantic owner.
            pub const fn $owner_method(&self) -> $owner {
                self.relationships.owner
            }

            /// Returns this field's source-order ordinal.
            pub const fn ordinal(&self) -> u32 {
                self.relationships.ordinal
            }

            /// Returns the cheap syntax-level default state.
            pub const fn default_presence(&self) -> RuntimeDefaultPresence {
                self.relationships.default_presence
            }

            /// Returns the synthesized provider identity when default syntax exists.
            pub const fn default_provider(&self) -> Option<$provider> {
                self.relationships.default_provider
            }
        }
    };
}

impl_defaultable_field!(
    StructFieldSymbol,
    structure,
    crate::StructSymbolId,
    crate::StructFieldDefaultProviderSymbolId
);

/// The synthesized receiver parameter of an instance-associated callable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceiverParameterSymbol {
    id: crate::ReceiverParameterSymbolId,
    key: SymbolKey,
    owner: crate::CallableSymbolId,
    imported: Option<ImportedSymbolBacking>,
}

impl ReceiverParameterSymbol {
    pub(crate) const fn new(
        id: crate::ReceiverParameterSymbolId,
        key: SymbolKey,
        owner: crate::CallableSymbolId,
    ) -> Self {
        Self {
            id,
            key,
            owner,
            imported: None,
        }
    }

    pub(crate) const fn new_imported(
        id: crate::ReceiverParameterSymbolId,
        key: SymbolKey,
        owner: crate::CallableSymbolId,
        imported: ImportedSymbolBacking,
    ) -> Self {
        Self {
            id,
            key,
            owner,
            imported: Some(imported),
        }
    }

    /// Returns this receiver's exact compilation-local ID.
    pub const fn id(&self) -> crate::ReceiverParameterSymbolId {
        self.id
    }

    /// Returns this receiver's deterministic synthesized key.
    pub const fn key(&self) -> &SymbolKey {
        &self.key
    }

    /// Returns this receiver's synthesized origin.
    pub const fn origin(&self) -> SymbolOrigin {
        if self.imported.is_some() {
            SymbolOrigin::Imported
        } else {
            SymbolOrigin::Synthesized
        }
    }

    /// Returns the callable that owns this receiver.
    pub const fn owner(&self) -> crate::CallableSymbolId {
        self.owner
    }

    /// Returns the receiver's fixed position before written parameters.
    pub const fn ordinal(&self) -> u32 {
        0
    }

    /// Returns the imported semantic-fact key, when available.
    pub fn imported_fact_key(
        &self,
    ) -> Option<ImportedSymbolFactKey<crate::ReceiverParameterSymbolId>> {
        self.imported.map(ImportedSymbolBacking::fact_key)
    }
}
impl_defaultable_field!(
    UnionPayloadFieldSymbol,
    variant,
    crate::UnionVariantSymbolId,
    crate::UnionPayloadDefaultProviderSymbolId
);
