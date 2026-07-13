use std::sync::Arc;

use bray_symbols::{
    BorrowKind, CallableAbi, CallableConstness, CallableExecution, CallableParameterMode,
    CallablePosition, CallableTrust,
};

use super::{
    InterfaceConstantTermId, InterfaceDependencyContractId, InterfaceGenericSubstitutionId,
    InterfaceTraitApplicationId, InterfaceTypeId,
};
use crate::InterfaceSymbolReference;

/// One callable parameter participating in durable callable type identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCallableParameter {
    pub(crate) name: Arc<str>,
    pub(crate) position: CallablePosition,
    pub(crate) mode: CallableParameterMode,
    pub(crate) ty: InterfaceTypeId,
}

impl InterfaceCallableParameter {
    /// Creates one callable type parameter.
    pub fn new(
        name: impl Into<Arc<str>>,
        position: CallablePosition,
        mode: CallableParameterMode,
        ty: InterfaceTypeId,
    ) -> Self {
        Self {
            name: name.into(),
            position,
            mode,
            ty,
        }
    }
}

/// Durable semantic type representation independent of compilation-local IDs.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceType {
    /// A named type definition with an ordered substitution.
    Named {
        /// Stable type definition reference.
        definition: InterfaceSymbolReference,
        /// Ordered generic substitution.
        substitution: InterfaceGenericSubstitutionId,
    },
    /// A generic type parameter.
    TypeParameter(InterfaceSymbolReference),
    /// The contextual `Self` type and its declaration context.
    ContextualSelf(InterfaceSymbolReference),
    /// An associated type projection.
    AssociatedTypeProjection {
        /// Applied trait supplying the member.
        application: InterfaceTraitApplicationId,
        /// Projected type member.
        member: InterfaceSymbolReference,
    },
    /// Ordered tuple elements.
    Tuple(Arc<[InterfaceTypeId]>),
    /// A fixed-size homogeneous array.
    Array {
        /// Element type.
        element: InterfaceTypeId,
        /// Checked open or closed length.
        length: InterfaceConstantTermId,
    },
    /// A dynamically sized slice.
    Slice(InterfaceTypeId),
    /// A nullable value.
    Nullable(InterfaceTypeId),
    /// A semantic borrow layer.
    Borrow {
        /// Borrow capability.
        kind: BorrowKind,
        /// Borrowed target.
        target: InterfaceTypeId,
    },
    /// A dynamically dispatched trait view.
    TraitView(InterfaceTraitApplicationId),
    /// Owned indirection through a storage-policy type.
    OwnedIndirection {
        /// Storage-policy type.
        storage: InterfaceTypeId,
        /// Owned target type.
        target: InterfaceTypeId,
    },
    /// A complete callable type.
    Callable {
        /// Ordered caller-visible parameters.
        parameters: Arc<[InterfaceCallableParameter]>,
        /// Result type.
        result: InterfaceTypeId,
        /// Constant-evaluation eligibility.
        constness: CallableConstness,
        /// Execution mode.
        execution: CallableExecution,
        /// Trust boundary.
        trust: CallableTrust,
        /// Calling convention.
        abi: CallableAbi,
        /// Portable dependency contract.
        dependency_contract: InterfaceDependencyContractId,
    },
}
