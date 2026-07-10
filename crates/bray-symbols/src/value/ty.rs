use std::sync::Arc;

use bray_base::{shared_slice, shared_str};

use crate::{GenericTypeParameterSymbolId, NamedTypeSymbolId, TraitTypeMemberSymbolId};

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

/// A validated semantic callable-parameter name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableParameterName(Arc<str>);

impl CallableParameterName {
    /// Creates a parameter name unless the canonical name is empty.
    pub fn try_new(name: impl Into<Arc<str>>) -> Option<Self> {
        let name = shared_str(name);

        if name.is_empty() {
            return None;
        }

        Some(Self(name))
    }

    /// Returns the canonical parameter name.
    pub fn as_str(&self) -> &str {
        &self.0
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
    execution: CallableExecution,
    trust: CallableTrust,
    abi: CallableAbi,
    dependency_contract: DependencyContractTemplateId,
}

impl CallableTypeData {
    /// Creates a callable type from every currently canonical caller-visible component.
    pub fn new(
        parameters: impl IntoIterator<Item = CallableParameterData>,
        result: TypeId,
        constness: CallableConstness,
        execution: CallableExecution,
        trust: CallableTrust,
        abi: CallableAbi,
        dependency_contract: DependencyContractTemplateId,
    ) -> Self {
        Self {
            parameters: shared_slice(parameters),
            result,
            constness,
            execution,
            trust,
            abi,
            dependency_contract,
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
        self.execution
    }

    /// Returns the callable trust boundary.
    pub const fn trust(&self) -> CallableTrust {
        self.trust
    }

    /// Returns the callable ABI.
    pub const fn abi(&self) -> CallableAbi {
        self.abi
    }

    /// Returns the portable caller-visible dependency contract.
    pub const fn dependency_contract(&self) -> DependencyContractTemplateId {
        self.dependency_contract
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
    /// A trait type member whose selected value remains context-dependent.
    AssociatedTypeProjection {
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
    use super::CallableParameterName;

    #[test]
    fn callable_parameter_names_reject_empty_text() {
        assert!(CallableParameterName::try_new("").is_none());

        let Some(name) = CallableParameterName::try_new("value") else {
            panic!("non-empty parameter name must be valid");
        };

        assert_eq!(name.as_str(), "value");
    }
}
