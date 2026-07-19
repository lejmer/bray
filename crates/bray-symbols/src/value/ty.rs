use std::sync::Arc;

use bray_base::shared_slice;

use crate::{
    AnySymbolId, GenericTypeParameterSymbolId, ImplementationSymbolId, NamedTypeSymbolId,
    SymbolKind, TraitSymbolId, TraitTypeMemberSymbolId,
};

use super::{
    ConstantTermId, DependencyContractTemplateId, GenericSubstitutionId, TraitApplicationId, TypeId,
};

/// The capability represented by one semantic borrow type layer.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BorrowKind {
    /// Shared observation through the borrow.
    Shared,
    /// Exclusive mutation authority through the borrow.
    Mutable,
}

/// The callable ABI participating in callable type identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableAbi {
    /// Bray's default compiler-defined ABI.
    Bray,
    /// The selected target's C ABI.
    C,
    /// The selected target's system ABI.
    System,
}

/// Whether a callable can run during constant evaluation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableConstness {
    /// Ordinary runtime-only evaluation.
    Runtime,
    /// Constant-evaluation eligibility.
    Constant,
}

/// A callable's execution mode.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableExecution {
    /// Immediate synchronous execution.
    Synchronous,
    /// Suspendable asynchronous execution.
    Asynchronous,
}

/// Portable dependency templates for invocation and deferred async execution.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableDependencyContracts {
    invocation: DependencyContractTemplateId,
    deferred_execution: Option<DependencyContractTemplateId>,
}

impl CallableDependencyContracts {
    /// Creates dependencies for an immediately executing callable.
    pub const fn synchronous(invocation: DependencyContractTemplateId) -> Self {
        Self {
            invocation,
            deferred_execution: None,
        }
    }

    /// Creates dependencies for lazy async invocation and deferred body execution.
    ///
    /// The deferred template is retained by `Future<T>` and transfers unchanged to `Task<T>`
    /// when the future is started.
    pub const fn asynchronous(
        invocation: DependencyContractTemplateId,
        deferred_execution: DependencyContractTemplateId,
    ) -> Self {
        Self {
            invocation,
            deferred_execution: Some(deferred_execution),
        }
    }

    /// Creates the phase shape required by the callable execution mode.
    pub const fn for_execution(
        execution: CallableExecution,
        invocation: DependencyContractTemplateId,
        deferred_execution: DependencyContractTemplateId,
    ) -> Self {
        match execution {
            CallableExecution::Synchronous => Self::synchronous(invocation),
            CallableExecution::Asynchronous => Self::asynchronous(invocation, deferred_execution),
        }
    }

    /// Returns dependencies incurred while invoking the callable.
    pub const fn invocation(self) -> DependencyContractTemplateId {
        self.invocation
    }

    /// Returns dependencies retained by deferred async execution.
    pub const fn deferred_execution(self) -> Option<DependencyContractTemplateId> {
        self.deferred_execution
    }

    /// Returns the callable execution mode implied by the phase shape.
    pub const fn execution(self) -> CallableExecution {
        match self.deferred_execution {
            Some(_) => CallableExecution::Asynchronous,
            None => CallableExecution::Synchronous,
        }
    }
}

/// A callable's trust boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableTrust {
    /// An ordinary safe callable.
    Safe,
    /// A callable with explicit trusted obligations.
    Trusted,
}

/// Whether a callable parameter can be supplied positionally.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallablePosition {
    /// The parameter must be supplied by name.
    NamedOnly,
    /// The parameter can be supplied positionally or by name.
    PositionalOrNamed,
}

/// Local mutation authority for an owned callable parameter.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableParameterMode {
    /// The parameter binding is immutable.
    Immutable,
    /// The parameter binding has mutable local authority.
    Mutable,
}

/// The declaration context that gives the contextual `Self` type its meaning.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SelfTypeContext {
    /// A member declared directly by one named structural type.
    NamedType(NamedTypeSymbolId),
    /// A member or constraint declared by one trait.
    Trait(TraitSymbolId),
    /// A member or constraint declared by one implementation.
    Implementation(ImplementationSymbolId),
}

impl SelfTypeContext {
    /// Classifies an exact symbol that can define contextual `Self`.
    pub const fn try_new(symbol: AnySymbolId) -> Option<Self> {
        match symbol {
            AnySymbolId::Struct(id) => Some(Self::NamedType(NamedTypeSymbolId::Struct(id))),
            AnySymbolId::Union(id) => Some(Self::NamedType(NamedTypeSymbolId::Union(id))),
            AnySymbolId::Trait(id) => Some(Self::Trait(id)),
            AnySymbolId::InherentImplementation(id) => {
                Some(Self::Implementation(ImplementationSymbolId::Inherent(id)))
            }
            AnySymbolId::UnnamedTraitImplementation(id) => Some(Self::Implementation(
                ImplementationSymbolId::UnnamedTrait(id),
            )),
            AnySymbolId::NamedTraitImplementation(id) => {
                Some(Self::Implementation(ImplementationSymbolId::NamedTrait(id)))
            }
            _ => None,
        }
    }

    /// Returns the exact declaration context retained by this type.
    pub fn symbol(self) -> AnySymbolId {
        match self {
            Self::NamedType(id) => id.into_any(),
            Self::Trait(id) => id.into(),
            Self::Implementation(id) => id.into_any(),
        }
    }

    /// Returns the exact symbol kind of the declaration context.
    pub const fn kind(self) -> SymbolKind {
        match self {
            Self::NamedType(id) => id.kind(),
            Self::Trait(id) => id.kind(),
            Self::Implementation(id) => id.kind(),
        }
    }
}

/// A validated semantic callable-parameter name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableParameterName(crate::SymbolName);

impl CallableParameterName {
    /// Creates a parameter name unless the canonical name is empty.
    pub fn try_new(name: impl Into<Arc<str>>) -> Option<Self> {
        crate::SymbolName::try_new(name).map(Self)
    }

    /// Returns the canonical parameter name.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// One ordered callable parameter participating in callable type identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableParameterData {
    name: CallableParameterName,
    position: CallablePosition,
    mode: CallableParameterMode,
    ty: TypeId,
}

impl CallableParameterData {
    /// Creates one semantic callable parameter.
    pub const fn new(
        name: CallableParameterName,
        position: CallablePosition,
        mode: CallableParameterMode,
        ty: TypeId,
    ) -> Self {
        Self {
            name,
            position,
            mode,
            ty,
        }
    }

    /// Returns the parameter name.
    pub const fn name(&self) -> &CallableParameterName {
        &self.name
    }

    /// Returns the parameter's positional-call permission.
    pub const fn position(&self) -> CallablePosition {
        self.position
    }

    /// Returns the owned binding's local mutation mode.
    pub const fn mode(&self) -> CallableParameterMode {
        self.mode
    }

    /// Returns the parameter type.
    pub const fn ty(&self) -> TypeId {
        self.ty
    }
}

/// Canonical callable type identity owned by the semantic value store.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableTypeData {
    parameters: Arc<[CallableParameterData]>,
    result: TypeId,
    constness: CallableConstness,
    trust: CallableTrust,
    abi: CallableAbi,
    dependency_contracts: CallableDependencyContracts,
}

impl CallableTypeData {
    /// Creates a callable type from every currently canonical caller-visible component.
    pub fn new(
        parameters: impl IntoIterator<Item = CallableParameterData>,
        result: TypeId,
        constness: CallableConstness,
        trust: CallableTrust,
        abi: CallableAbi,
        dependency_contracts: CallableDependencyContracts,
    ) -> Self {
        Self {
            parameters: shared_slice(parameters),
            result,
            constness,
            trust,
            abi,
            dependency_contracts,
        }
    }

    /// Returns the ordered parameter surface.
    pub fn parameters(&self) -> &[CallableParameterData] {
        &self.parameters
    }

    /// Returns the callable result type.
    pub const fn result(&self) -> TypeId {
        self.result
    }

    /// Returns the callable's constant-evaluation contract.
    pub const fn constness(&self) -> CallableConstness {
        self.constness
    }

    /// Returns the callable execution mode.
    pub const fn execution(&self) -> CallableExecution {
        self.dependency_contracts.execution()
    }

    /// Returns the callable trust boundary.
    pub const fn trust(&self) -> CallableTrust {
        self.trust
    }

    /// Returns the callable ABI.
    pub const fn abi(&self) -> CallableAbi {
        self.abi
    }

    /// Returns invocation and deferred-execution dependency templates.
    pub const fn dependency_contracts(&self) -> CallableDependencyContracts {
        self.dependency_contracts
    }
}

/// The closed durable representation of one canonical semantic type.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TypeData {
    /// The single canonical recovery type.
    Error,
    /// A named structural type with ordered generic arguments.
    Named {
        /// The exact type definition.
        definition: NamedTypeSymbolId,
        /// The ordered generic substitution.
        substitution: GenericSubstitutionId,
    },
    /// A generic type parameter.
    TypeParameter(GenericTypeParameterSymbolId),
    /// The contextual `Self` type tied to its declaration context.
    ContextualSelf(SelfTypeContext),
    /// A trait type member whose selected value remains context-dependent.
    AssociatedTypeProjection {
        /// The type whose implementation supplies the member.
        subject: TypeId,
        /// The exact applied trait.
        application: TraitApplicationId,
        /// The projected type-valued member.
        member: TraitTypeMemberSymbolId,
    },
    /// An ordered structural tuple.
    Tuple(Arc<[TypeId]>),
    /// A fixed-size homogeneous array.
    Array {
        /// The element type.
        element: TypeId,
        /// The checked open or closed length term.
        length: ConstantTermId,
    },
    /// A dynamically sized homogeneous slice.
    Slice(TypeId),
    /// A nullable value type.
    Nullable(TypeId),
    /// One borrow layer.
    Borrow {
        /// Shared or mutable capability.
        kind: BorrowKind,
        /// The borrowed target type.
        target: TypeId,
    },
    /// A dynamically dispatched trait view.
    TraitView(TraitApplicationId),
    /// Owned indirection through an explicit storage-policy type.
    OwnedIndirection {
        /// The storage-policy type.
        storage: TypeId,
        /// The owned target type.
        target: TypeId,
    },
    /// A complete semantic callable type.
    Callable(CallableTypeData),
}

impl TypeData {
    /// Creates an ordered tuple type.
    pub fn tuple(elements: impl IntoIterator<Item = TypeId>) -> Self {
        Self::Tuple(shared_slice(elements))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CallableDependencyContracts, CallableExecution, CallableParameterName, SelfTypeContext,
    };
    use crate::{
        AnySymbolId, DependencyContractTemplateData, FunctionSymbolId, SemanticValueStore,
        StructSymbolId, SymbolId, SymbolKind, TraitSymbolId,
    };

    #[test]
    fn callable_parameter_names_reject_empty_text() {
        assert!(CallableParameterName::try_new("").is_none());

        let Some(name) = CallableParameterName::try_new("value") else {
            panic!("non-empty parameter name must be valid");
        };

        assert_eq!(name.as_str(), "value");
    }

    #[test]
    fn async_callable_dependencies_retain_a_distinct_deferred_phase() {
        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("semantic value store must be available");
        };

        let Ok(dependencies) =
            store.intern_dependency_contract_template(DependencyContractTemplateData::new([]))
        else {
            panic!("empty dependency contract must be valid");
        };

        let synchronous = CallableDependencyContracts::synchronous(dependencies);
        let asynchronous = CallableDependencyContracts::asynchronous(dependencies, dependencies);

        assert_eq!(synchronous.execution(), CallableExecution::Synchronous);
        assert_eq!(synchronous.deferred_execution(), None);
        assert_eq!(asynchronous.execution(), CallableExecution::Asynchronous);
        assert_eq!(asynchronous.deferred_execution(), Some(dependencies));
    }

    #[test]
    fn contextual_self_accepts_only_declaration_contexts() {
        let structure = StructSymbolId::from_symbol_id(SymbolId::new(1));
        let trait_symbol = TraitSymbolId::from_symbol_id(SymbolId::new(2));

        let Some(structure_context) = SelfTypeContext::try_new(structure.into()) else {
            panic!("named types must define contextual Self");
        };

        let Some(trait_context) = SelfTypeContext::try_new(trait_symbol.into()) else {
            panic!("traits must define contextual Self");
        };

        assert_eq!(structure_context.symbol(), AnySymbolId::Struct(structure));
        assert_eq!(structure_context.kind(), SymbolKind::Struct);
        assert_eq!(trait_context.symbol(), AnySymbolId::Trait(trait_symbol));
        assert_eq!(trait_context.kind(), SymbolKind::Trait);

        let function = FunctionSymbolId::from_symbol_id(SymbolId::new(3));

        assert!(SelfTypeContext::try_new(function.into()).is_none());
    }
}
