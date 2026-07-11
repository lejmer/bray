macro_rules! for_each_compilation_symbol_kind {
    ($consumer:ident) => {
        $consumer! {
            "The root of compiler-known declarations.";
            CompilerKnownEnvironmentSymbolId => CompilerKnownEnvironment: CompilerKnownEnvironment,
            "A compiled or referenced package.";
            PackageSymbolId => Package: Package,
            "A logical module.";
            ModuleSymbolId => Module: Module,
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
                supports_interface_generic_parameters(owner)
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

const fn supports_interface_generic_parameters(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Function
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
            | SymbolKind::TypeCallableMember
            | SymbolKind::Constructor
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
    use super::SymbolKind;

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
}
