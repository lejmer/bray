macro_rules! for_each_compilation_symbol_kind {
    ($consumer:ident) => {
        $consumer! {
            "The root of compiler-known declarations.";
            CompilerKnownEnvironmentSymbolId => CompilerKnownEnvironment: CompilerKnownEnvironment,
            "A compiled or referenced package.";
            PackageSymbolId => Package: Package,
            "A logical module.";
            ModuleSymbolId => Module: Module,
            "A compiler-known capability that may authorize trusted implementation behavior.";
            TrustedCapabilitySymbolId => TrustedCapability: TrustedCapability,
            "A constant declaration.";
            ConstantSymbolId => Constant: Constant,
            "A function declaration.";
            FunctionSymbolId => Function: Function,
            "A predicate declaration.";
            PredicateSymbolId => Predicate: Predicate,
            "A callable contract declaration.";
            CallableContractSymbolId => CallableContract: CallableContract,
            "A callable overload declaration.";
            CallableOverloadSymbolId => CallableOverload: CallableOverload,
            "An implementation overload declaration.";
            ImplementationOverloadSymbolId => ImplementationOverload: ImplementationOverload,
            "A struct declaration.";
            StructSymbolId => Struct: Struct,
            "A union declaration.";
            UnionSymbolId => Union: Union,
            "A trait declaration.";
            TraitSymbolId => Trait: Trait,
            "An inherent implementation declaration.";
            InherentImplementationSymbolId => InherentImplementation: InherentImplementation,
            "An unnamed trait implementation declaration.";
            UnnamedTraitImplementationSymbolId => UnnamedTraitImplementation: UnnamedTraitImplementation,
            "A named trait implementation declaration.";
            NamedTraitImplementationSymbolId => NamedTraitImplementation: NamedTraitImplementation,
            "A struct field declaration.";
            StructFieldSymbolId => StructField: StructField,
            "A union variant declaration.";
            UnionVariantSymbolId => UnionVariant: UnionVariant,
            "A field in a union variant payload.";
            UnionPayloadFieldSymbolId => UnionPayloadField: UnionPayloadField,
            "A callable member declared by a type.";
            TypeCallableMemberSymbolId => TypeCallableMember: TypeCallableMember,
            "A constructor declaration.";
            ConstructorSymbolId => Constructor: Constructor,
            "A finalizer declaration.";
            FinalizerSymbolId => Finalizer: Finalizer,
            "A destructor declaration.";
            DestructorSymbolId => Destructor: Destructor,
            "A scope-enter lifecycle declaration.";
            ScopeEnterSymbolId => ScopeEnter: ScopeEnter,
            "A scope-exit lifecycle declaration.";
            ScopeExitSymbolId => ScopeExit: ScopeExit,
            "A type-valued member declared by a type.";
            InherentTypeMemberSymbolId => InherentTypeMember: InherentTypeMember,
            "A callable member required by a trait.";
            TraitCallableMemberSymbolId => TraitCallableMember: TraitCallableMember,
            "A constant member required by a trait.";
            TraitConstantMemberSymbolId => TraitConstantMember: TraitConstantMember,
            "A type-valued member required by a trait.";
            TraitTypeMemberSymbolId => TraitTypeMember: TraitTypeMember,
            "A predicate member required by a trait.";
            TraitPredicateMemberSymbolId => TraitPredicateMember: TraitPredicateMember,
            "A finalizer requirement declared by a trait.";
            TraitFinalizerRequirementSymbolId => TraitFinalizerRequirement: TraitFinalizerRequirement,
            "A destructor requirement declared by a trait.";
            TraitDestructorRequirementSymbolId => TraitDestructorRequirement: TraitDestructorRequirement,
            "A scope-enter requirement declared by a trait.";
            TraitScopeEnterRequirementSymbolId => TraitScopeEnterRequirement: TraitScopeEnterRequirement,
            "A scope-exit requirement declared by a trait.";
            TraitScopeExitRequirementSymbolId => TraitScopeExitRequirement: TraitScopeExitRequirement,
            "A callable member fulfilling a trait requirement.";
            TraitCallableFulfillmentSymbolId => TraitCallableFulfillment: TraitCallableFulfillment,
            "A constant member fulfilling a trait requirement.";
            TraitConstantFulfillmentSymbolId => TraitConstantFulfillment: TraitConstantFulfillment,
            "A type-valued member fulfilling a trait requirement.";
            TraitTypeFulfillmentSymbolId => TraitTypeFulfillment: TraitTypeFulfillment,
            "A predicate member fulfilling a trait requirement.";
            TraitPredicateFulfillmentSymbolId => TraitPredicateFulfillment: TraitPredicateFulfillment,
            "A scope-enter member fulfilling a trait requirement.";
            TraitScopeEnterFulfillmentSymbolId => TraitScopeEnterFulfillment: TraitScopeEnterFulfillment,
            "A scope-exit member fulfilling a trait requirement.";
            TraitScopeExitFulfillmentSymbolId => TraitScopeExitFulfillment: TraitScopeExitFulfillment,
            "A generic type parameter.";
            GenericTypeParameterSymbolId => GenericTypeParameter: GenericTypeParameter,
            "A generic constant parameter.";
            GenericConstParameterSymbolId => GenericConstParameter: GenericConstParameter,
            "A callable parameter.";
            CallableParameterSymbolId => CallableParameter: CallableParameter,
            "A predicate parameter.";
            PredicateParameterSymbolId => PredicateParameter: PredicateParameter,
            "A compiler-introduced receiver parameter.";
            ReceiverParameterSymbolId => ReceiverParameter: ReceiverParameter,
            "A synthesized provider for a callable parameter default.";
            CallableParameterDefaultProviderSymbolId => CallableParameterDefaultProvider: CallableParameterDefaultProvider,
            "A synthesized provider for a struct field default.";
            StructFieldDefaultProviderSymbolId => StructFieldDefaultProvider: StructFieldDefaultProvider,
            "A synthesized provider for a union payload field default.";
            UnionPayloadDefaultProviderSymbolId => UnionPayloadDefaultProvider: UnionPayloadDefaultProvider,
        }
    };
}

pub(crate) use for_each_compilation_symbol_kind;

macro_rules! define_symbol_kind {
    ($($documentation:literal; $id:ident => $variant:ident : $kind:ident),+ $(,)?) => {
        /// Classifies a semantic symbol without identifying a particular symbol instance.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum SymbolKind {
            $(
                #[doc = $documentation]
                $kind,
            )+
            /// A body-local binding.
            LocalBinding,
            /// A body-local constant.
            LocalConstant,
            /// An anonymous callable in a body.
            AnonymousCallable,
            /// A parameter of an anonymous callable.
            AnonymousCallableParameter,
            /// A contextual result binding in a postcondition.
            PostconditionResult,
        }
    };
}

for_each_compilation_symbol_kind!(define_symbol_kind);

impl SymbolKind {
    /// Returns this symbol kind's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CompilerKnownEnvironment => "compiler_known_environment",
            Self::Package => "package",
            Self::Module => "module",
            Self::TrustedCapability => "trusted_capability",
            Self::Constant => "constant",
            Self::Function => "function",
            Self::Predicate => "predicate",
            Self::CallableContract => "callable_contract",
            Self::CallableOverload => "callable_overload",
            Self::ImplementationOverload => "implementation_overload",
            Self::Struct => "struct",
            Self::Union => "union",
            Self::Trait => "trait",
            Self::InherentImplementation => "inherent_implementation",
            Self::UnnamedTraitImplementation => "unnamed_trait_implementation",
            Self::NamedTraitImplementation => "named_trait_implementation",
            Self::StructField => "struct_field",
            Self::UnionVariant => "union_variant",
            Self::UnionPayloadField => "union_payload_field",
            Self::TypeCallableMember => "type_callable_member",
            Self::Constructor => "constructor",
            Self::Finalizer => "finalizer",
            Self::Destructor => "destructor",
            Self::ScopeEnter => "scope_enter",
            Self::ScopeExit => "scope_exit",
            Self::InherentTypeMember => "inherent_type_valued_member",
            Self::TraitCallableMember => "trait_callable_member",
            Self::TraitConstantMember => "trait_constant_member",
            Self::TraitTypeMember => "trait_type_valued_member",
            Self::TraitPredicateMember => "trait_predicate_member",
            Self::TraitFinalizerRequirement => "trait_finalizer_requirement",
            Self::TraitDestructorRequirement => "trait_destructor_requirement",
            Self::TraitScopeEnterRequirement => "trait_scope_enter_requirement",
            Self::TraitScopeExitRequirement => "trait_scope_exit_requirement",
            Self::TraitCallableFulfillment => "trait_callable_fulfillment",
            Self::TraitConstantFulfillment => "trait_constant_fulfillment",
            Self::TraitTypeFulfillment => "trait_type_valued_fulfillment",
            Self::TraitPredicateFulfillment => "trait_predicate_fulfillment",
            Self::TraitScopeEnterFulfillment => "trait_scope_enter_fulfillment",
            Self::TraitScopeExitFulfillment => "trait_scope_exit_fulfillment",
            Self::GenericTypeParameter => "generic_type_parameter",
            Self::GenericConstParameter => "generic_const_parameter",
            Self::CallableParameter => "callable_parameter",
            Self::PredicateParameter => "predicate_parameter",
            Self::ReceiverParameter => "receiver_parameter",
            Self::CallableParameterDefaultProvider => "callable_parameter_default_provider",
            Self::StructFieldDefaultProvider => "struct_field_default_provider",
            Self::UnionPayloadDefaultProvider => "union_payload_default_provider",
            Self::LocalBinding => "local_binding",
            Self::LocalConstant => "local_constant",
            Self::AnonymousCallable => "anonymous_callable",
            Self::AnonymousCallableParameter => "anonymous_callable_parameter",
            Self::PostconditionResult => "postcondition_result",
        }
    }

    /// Returns whether symbols of this kind use the body-local identity space.
    pub const fn is_local(self) -> bool {
        matches!(
            self,
            Self::LocalBinding
                | Self::LocalConstant
                | Self::AnonymousCallable
                | Self::AnonymousCallableParameter
                | Self::PostconditionResult
        )
    }

    /// Returns whether symbols of this kind receive compilation-wide symbol IDs.
    pub const fn has_compilation_wide_id(self) -> bool {
        !self.is_local()
    }

    /// Returns whether this kind can be introduced directly by a source declaration.
    pub const fn can_be_source_declared(self) -> bool {
        !matches!(
            self,
            Self::CompilerKnownEnvironment
                | Self::Package
                | Self::Module
                | Self::ReceiverParameter
                | Self::CallableParameterDefaultProvider
                | Self::StructFieldDefaultProvider
                | Self::UnionPayloadDefaultProvider
        ) && !self.is_local()
    }

    /// Returns whether this kind denotes a declared callable or lifecycle operation.
    pub const fn is_callable(self) -> bool {
        matches!(
            self,
            Self::Function
                | Self::TypeCallableMember
                | Self::TraitCallableMember
                | Self::TraitCallableFulfillment
                | Self::Constructor
                | Self::Finalizer
                | Self::Destructor
                | Self::ScopeEnter
                | Self::ScopeExit
                | Self::TraitFinalizerRequirement
                | Self::TraitDestructorRequirement
                | Self::TraitScopeEnterRequirement
                | Self::TraitScopeExitRequirement
                | Self::TraitScopeEnterFulfillment
                | Self::TraitScopeExitFulfillment
        )
    }

    /// Returns whether this kind denotes an inherent or trait implementation declaration.
    pub const fn is_implementation(self) -> bool {
        matches!(
            self,
            Self::InherentImplementation
                | Self::UnnamedTraitImplementation
                | Self::NamedTraitImplementation
        )
    }

    /// Returns whether declarations of this kind may introduce generic parameters.
    pub const fn admits_generic_parameters(self) -> bool {
        matches!(
            self,
            Self::Function
                | Self::Predicate
                | Self::CallableContract
                | Self::CallableOverload
                | Self::ImplementationOverload
                | Self::Struct
                | Self::Union
                | Self::Trait
                | Self::InherentImplementation
                | Self::UnnamedTraitImplementation
                | Self::NamedTraitImplementation
                | Self::TypeCallableMember
                | Self::Constructor
                | Self::TraitCallableMember
                | Self::TraitCallableFulfillment
        )
    }

    /// Returns whether this kind may be instantiated through generic substitution.
    pub const fn supports_generic_substitutions(self) -> bool {
        matches!(
            self,
            Self::Constant
                | Self::Function
                | Self::Predicate
                | Self::CallableContract
                | Self::Struct
                | Self::Union
                | Self::Trait
                | Self::InherentImplementation
                | Self::UnnamedTraitImplementation
                | Self::NamedTraitImplementation
                | Self::TypeCallableMember
                | Self::Constructor
                | Self::Finalizer
                | Self::Destructor
                | Self::ScopeEnter
                | Self::ScopeExit
                | Self::InherentTypeMember
                | Self::TraitCallableMember
                | Self::TraitConstantMember
                | Self::TraitTypeMember
                | Self::TraitPredicateMember
                | Self::TraitFinalizerRequirement
                | Self::TraitDestructorRequirement
                | Self::TraitScopeEnterRequirement
                | Self::TraitScopeExitRequirement
                | Self::TraitCallableFulfillment
                | Self::TraitConstantFulfillment
                | Self::TraitTypeFulfillment
                | Self::TraitPredicateFulfillment
                | Self::TraitScopeEnterFulfillment
                | Self::TraitScopeExitFulfillment
        )
    }
}

/// Closed typed relationship between ordinary semantic symbols.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SymbolRelationshipKind {
    /// A logical module exposed by a package root.
    PackageModule,
    /// A module-level declaration exposed by a logical module.
    ModuleMember,
    /// A callable, lifecycle, or type-valued member owned by a product type.
    TypeMember,
    /// A requirement member owned by a trait.
    TraitMember,
    /// A declaration member owned by an inherent implementation.
    ImplementationMember,
    /// A field owned by a struct.
    StructField,
    /// A variant owned by a union.
    UnionVariant,
    /// A payload field owned by a union variant.
    UnionPayloadField,
    /// A generic type or constant parameter owned by a generic declaration.
    GenericParameter,
    /// A callable or receiver parameter owned by a callable declaration.
    CallableParameter,
    /// A predicate parameter owned by a predicate declaration.
    PredicateParameter,
    /// An independently declared arm referenced by an overload family.
    OverloadArm,
    /// A typed trait-member fulfillment owned by an implementation.
    ImplementationFulfillment,
    /// A synthesized runtime-default provider owned by its subject.
    DefaultProvider,
}

impl SymbolRelationshipKind {
    /// Every closed symbol relationship in stable presentation order.
    pub const ALL: [Self; 14] = [
        Self::PackageModule,
        Self::ModuleMember,
        Self::TypeMember,
        Self::TraitMember,
        Self::ImplementationMember,
        Self::StructField,
        Self::UnionVariant,
        Self::UnionPayloadField,
        Self::GenericParameter,
        Self::CallableParameter,
        Self::PredicateParameter,
        Self::OverloadArm,
        Self::ImplementationFulfillment,
        Self::DefaultProvider,
    ];

    /// Returns this relationship kind's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PackageModule => "package_module",
            Self::ModuleMember => "module_member",
            Self::TypeMember => "type_member",
            Self::TraitMember => "trait_member",
            Self::ImplementationMember => "implementation_member",
            Self::StructField => "struct_field",
            Self::UnionVariant => "union_variant",
            Self::UnionPayloadField => "union_payload_field",
            Self::GenericParameter => "generic_parameter",
            Self::CallableParameter => "callable_parameter",
            Self::PredicateParameter => "predicate_parameter",
            Self::OverloadArm => "overload_arm",
            Self::ImplementationFulfillment => "implementation_fulfillment",
            Self::DefaultProvider => "default_provider",
        }
    }

    /// Classifies a supported owner-member pair.
    pub fn between(owner: SymbolKind, member: SymbolKind) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|relationship| relationship.supports(owner, member))
    }

    /// Returns whether the owner and member categories satisfy this relationship contract.
    pub fn supports(self, owner: SymbolKind, member: SymbolKind) -> bool {
        match self {
            Self::PackageModule => owner == SymbolKind::Package && member == SymbolKind::Module,
            Self::ModuleMember => owner == SymbolKind::Module && is_module_member(member),
            Self::TypeMember => {
                matches!(owner, SymbolKind::Struct | SymbolKind::Union)
                    && is_type_or_implementation_member(member)
            }
            Self::TraitMember => owner == SymbolKind::Trait && is_trait_member(member),
            Self::ImplementationMember => {
                owner == SymbolKind::InherentImplementation
                    && is_type_or_implementation_member(member)
            }
            Self::StructField => owner == SymbolKind::Struct && member == SymbolKind::StructField,
            Self::UnionVariant => owner == SymbolKind::Union && member == SymbolKind::UnionVariant,
            Self::UnionPayloadField => {
                owner == SymbolKind::UnionVariant && member == SymbolKind::UnionPayloadField
            }
            Self::GenericParameter => {
                owner.admits_generic_parameters()
                    && matches!(
                        member,
                        SymbolKind::GenericTypeParameter | SymbolKind::GenericConstParameter
                    )
            }
            Self::CallableParameter => {
                is_interface_callable(owner)
                    && matches!(
                        member,
                        SymbolKind::CallableParameter | SymbolKind::ReceiverParameter
                    )
            }
            Self::PredicateParameter => {
                owner == SymbolKind::Predicate && member == SymbolKind::PredicateParameter
            }
            Self::OverloadArm => matches!(
                owner,
                SymbolKind::CallableOverload | SymbolKind::ImplementationOverload
            ),
            Self::ImplementationFulfillment => {
                matches!(
                    owner,
                    SymbolKind::NamedTraitImplementation | SymbolKind::UnnamedTraitImplementation
                ) && is_trait_fulfillment(member)
            }
            Self::DefaultProvider => is_default_provider_pair(owner, member),
        }
    }
}

const fn is_module_member(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Constant
            | SymbolKind::Function
            | SymbolKind::Predicate
            | SymbolKind::CallableContract
            | SymbolKind::CallableOverload
            | SymbolKind::ImplementationOverload
            | SymbolKind::Struct
            | SymbolKind::Union
            | SymbolKind::Trait
            | SymbolKind::InherentImplementation
            | SymbolKind::UnnamedTraitImplementation
            | SymbolKind::NamedTraitImplementation
    )
}

const fn is_type_or_implementation_member(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::TypeCallableMember
            | SymbolKind::Constructor
            | SymbolKind::Finalizer
            | SymbolKind::Destructor
            | SymbolKind::ScopeEnter
            | SymbolKind::ScopeExit
            | SymbolKind::InherentTypeMember
    )
}

const fn is_trait_member(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::TraitCallableMember
            | SymbolKind::TraitConstantMember
            | SymbolKind::TraitTypeMember
            | SymbolKind::TraitPredicateMember
            | SymbolKind::TraitFinalizerRequirement
            | SymbolKind::TraitDestructorRequirement
            | SymbolKind::TraitScopeEnterRequirement
            | SymbolKind::TraitScopeExitRequirement
    )
}

const fn is_interface_callable(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Function
            | SymbolKind::CallableContract
            | SymbolKind::TypeCallableMember
            | SymbolKind::TraitCallableMember
            | SymbolKind::TraitCallableFulfillment
            | SymbolKind::Constructor
            | SymbolKind::Finalizer
            | SymbolKind::Destructor
            | SymbolKind::ScopeEnter
            | SymbolKind::ScopeExit
            | SymbolKind::TraitFinalizerRequirement
            | SymbolKind::TraitDestructorRequirement
            | SymbolKind::TraitScopeEnterRequirement
            | SymbolKind::TraitScopeExitRequirement
    )
}

const fn is_trait_fulfillment(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::TraitCallableFulfillment
            | SymbolKind::TraitConstantFulfillment
            | SymbolKind::TraitTypeFulfillment
            | SymbolKind::TraitPredicateFulfillment
            | SymbolKind::TraitScopeEnterFulfillment
            | SymbolKind::TraitScopeExitFulfillment
    )
}

const fn is_default_provider_pair(owner: SymbolKind, member: SymbolKind) -> bool {
    matches!(
        (owner, member),
        (
            SymbolKind::CallableParameter,
            SymbolKind::CallableParameterDefaultProvider
        ) | (
            SymbolKind::StructField,
            SymbolKind::StructFieldDefaultProvider
        ) | (
            SymbolKind::UnionPayloadField,
            SymbolKind::UnionPayloadDefaultProvider
        )
    )
}

#[cfg(test)]
mod tests {
    use super::{SymbolKind, SymbolRelationshipKind};

    #[test]
    fn local_kinds_use_a_separate_identity_space() {
        let local_kinds = [
            SymbolKind::LocalBinding,
            SymbolKind::LocalConstant,
            SymbolKind::AnonymousCallable,
            SymbolKind::AnonymousCallableParameter,
            SymbolKind::PostconditionResult,
        ];

        for kind in local_kinds {
            assert!(kind.is_local());
            assert!(!kind.has_compilation_wide_id());
            assert!(!kind.can_be_source_declared());
        }
    }

    #[test]
    fn source_and_synthesized_kinds_are_distinguished() {
        assert!(SymbolKind::Function.can_be_source_declared());
        assert!(SymbolKind::GenericTypeParameter.can_be_source_declared());
        assert!(!SymbolKind::ReceiverParameter.can_be_source_declared());
        assert!(SymbolKind::ReceiverParameter.has_compilation_wide_id());
    }

    #[test]
    fn relationship_support_matches_every_accepted_owner_member_pair() {
        let kinds = all_symbol_kinds();

        for relationship in SymbolRelationshipKind::ALL {
            let expected = accepted_pairs(relationship, &kinds);

            for owner in &kinds {
                for member in &kinds {
                    assert_eq!(
                        relationship.supports(*owner, *member),
                        expected.contains(&(*owner, *member)),
                        "unexpected {relationship:?} support for {owner:?} -> {member:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn symbol_and_relationship_kinds_have_stable_names() {
        assert_eq!(
            SymbolKind::TraitTypeMember.as_str(),
            "trait_type_valued_member"
        );

        assert_eq!(
            SymbolRelationshipKind::GenericParameter.as_str(),
            "generic_parameter"
        );
    }

    #[test]
    fn relationship_support_retains_trait_callable_generics_and_lifecycle_parameters() {
        for owner in [
            SymbolKind::TraitCallableMember,
            SymbolKind::TraitCallableFulfillment,
        ] {
            assert!(
                SymbolRelationshipKind::GenericParameter
                    .supports(owner, SymbolKind::GenericTypeParameter)
            );

            assert!(
                SymbolRelationshipKind::GenericParameter
                    .supports(owner, SymbolKind::GenericConstParameter)
            );
        }

        for owner in [
            SymbolKind::Finalizer,
            SymbolKind::Destructor,
            SymbolKind::ScopeEnter,
            SymbolKind::ScopeExit,
            SymbolKind::TraitFinalizerRequirement,
            SymbolKind::TraitDestructorRequirement,
            SymbolKind::TraitScopeEnterRequirement,
            SymbolKind::TraitScopeExitRequirement,
        ] {
            assert!(
                SymbolRelationshipKind::CallableParameter
                    .supports(owner, SymbolKind::CallableParameter)
            );

            assert!(
                SymbolRelationshipKind::CallableParameter
                    .supports(owner, SymbolKind::ReceiverParameter)
            );
        }
    }

    fn accepted_pairs(
        relationship: SymbolRelationshipKind,
        all_kinds: &[SymbolKind],
    ) -> Vec<(SymbolKind, SymbolKind)> {
        match relationship {
            SymbolRelationshipKind::PackageModule => {
                vec![(SymbolKind::Package, SymbolKind::Module)]
            }
            SymbolRelationshipKind::ModuleMember => cross(
                &[SymbolKind::Module],
                &[
                    SymbolKind::Constant,
                    SymbolKind::Function,
                    SymbolKind::Predicate,
                    SymbolKind::CallableContract,
                    SymbolKind::CallableOverload,
                    SymbolKind::ImplementationOverload,
                    SymbolKind::Struct,
                    SymbolKind::Union,
                    SymbolKind::Trait,
                    SymbolKind::InherentImplementation,
                    SymbolKind::UnnamedTraitImplementation,
                    SymbolKind::NamedTraitImplementation,
                ],
            ),
            SymbolRelationshipKind::TypeMember => {
                cross(&[SymbolKind::Struct, SymbolKind::Union], &type_members())
            }
            SymbolRelationshipKind::TraitMember => cross(
                &[SymbolKind::Trait],
                &[
                    SymbolKind::TraitCallableMember,
                    SymbolKind::TraitConstantMember,
                    SymbolKind::TraitTypeMember,
                    SymbolKind::TraitPredicateMember,
                    SymbolKind::TraitFinalizerRequirement,
                    SymbolKind::TraitDestructorRequirement,
                    SymbolKind::TraitScopeEnterRequirement,
                    SymbolKind::TraitScopeExitRequirement,
                ],
            ),
            SymbolRelationshipKind::ImplementationMember => {
                cross(&[SymbolKind::InherentImplementation], &type_members())
            }
            SymbolRelationshipKind::StructField => {
                vec![(SymbolKind::Struct, SymbolKind::StructField)]
            }
            SymbolRelationshipKind::UnionVariant => {
                vec![(SymbolKind::Union, SymbolKind::UnionVariant)]
            }
            SymbolRelationshipKind::UnionPayloadField => {
                vec![(SymbolKind::UnionVariant, SymbolKind::UnionPayloadField)]
            }
            SymbolRelationshipKind::GenericParameter => cross(
                &generic_owners(),
                &[
                    SymbolKind::GenericTypeParameter,
                    SymbolKind::GenericConstParameter,
                ],
            ),
            SymbolRelationshipKind::CallableParameter => cross(
                &callable_owners(),
                &[SymbolKind::CallableParameter, SymbolKind::ReceiverParameter],
            ),
            SymbolRelationshipKind::PredicateParameter => {
                vec![(SymbolKind::Predicate, SymbolKind::PredicateParameter)]
            }
            SymbolRelationshipKind::OverloadArm => cross(
                &[
                    SymbolKind::CallableOverload,
                    SymbolKind::ImplementationOverload,
                ],
                all_kinds,
            ),
            SymbolRelationshipKind::ImplementationFulfillment => cross(
                &[
                    SymbolKind::NamedTraitImplementation,
                    SymbolKind::UnnamedTraitImplementation,
                ],
                &[
                    SymbolKind::TraitCallableFulfillment,
                    SymbolKind::TraitConstantFulfillment,
                    SymbolKind::TraitTypeFulfillment,
                    SymbolKind::TraitPredicateFulfillment,
                    SymbolKind::TraitScopeEnterFulfillment,
                    SymbolKind::TraitScopeExitFulfillment,
                ],
            ),
            SymbolRelationshipKind::DefaultProvider => vec![
                (
                    SymbolKind::CallableParameter,
                    SymbolKind::CallableParameterDefaultProvider,
                ),
                (
                    SymbolKind::StructField,
                    SymbolKind::StructFieldDefaultProvider,
                ),
                (
                    SymbolKind::UnionPayloadField,
                    SymbolKind::UnionPayloadDefaultProvider,
                ),
            ],
        }
    }

    fn type_members() -> [SymbolKind; 7] {
        [
            SymbolKind::TypeCallableMember,
            SymbolKind::Constructor,
            SymbolKind::Finalizer,
            SymbolKind::Destructor,
            SymbolKind::ScopeEnter,
            SymbolKind::ScopeExit,
            SymbolKind::InherentTypeMember,
        ]
    }

    fn generic_owners() -> [SymbolKind; 15] {
        [
            SymbolKind::Function,
            SymbolKind::Predicate,
            SymbolKind::CallableContract,
            SymbolKind::CallableOverload,
            SymbolKind::ImplementationOverload,
            SymbolKind::Struct,
            SymbolKind::Union,
            SymbolKind::Trait,
            SymbolKind::InherentImplementation,
            SymbolKind::UnnamedTraitImplementation,
            SymbolKind::NamedTraitImplementation,
            SymbolKind::TypeCallableMember,
            SymbolKind::Constructor,
            SymbolKind::TraitCallableMember,
            SymbolKind::TraitCallableFulfillment,
        ]
    }

    fn callable_owners() -> [SymbolKind; 14] {
        [
            SymbolKind::Function,
            SymbolKind::CallableContract,
            SymbolKind::TypeCallableMember,
            SymbolKind::TraitCallableMember,
            SymbolKind::TraitCallableFulfillment,
            SymbolKind::Constructor,
            SymbolKind::Finalizer,
            SymbolKind::Destructor,
            SymbolKind::ScopeEnter,
            SymbolKind::ScopeExit,
            SymbolKind::TraitFinalizerRequirement,
            SymbolKind::TraitDestructorRequirement,
            SymbolKind::TraitScopeEnterRequirement,
            SymbolKind::TraitScopeExitRequirement,
        ]
    }

    fn cross(owners: &[SymbolKind], members: &[SymbolKind]) -> Vec<(SymbolKind, SymbolKind)> {
        owners
            .iter()
            .flat_map(|owner| members.iter().map(move |member| (*owner, *member)))
            .collect()
    }

    fn all_symbol_kinds() -> Vec<SymbolKind> {
        macro_rules! collect_compilation_kinds {
            ($($documentation:literal; $id:ident => $variant:ident : $kind:ident),+ $(,)?) => {
                vec![$(SymbolKind::$kind),+]
            };
        }

        let mut kinds = for_each_compilation_symbol_kind!(collect_compilation_kinds);

        kinds.extend([
            SymbolKind::LocalBinding,
            SymbolKind::LocalConstant,
            SymbolKind::AnonymousCallable,
            SymbolKind::AnonymousCallableParameter,
            SymbolKind::PostconditionResult,
        ]);

        kinds
    }
}
