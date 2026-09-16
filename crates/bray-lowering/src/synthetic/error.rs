use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirCapacityError, MirGeneratedLifecycleRole, MirHelperReference,
};
use bray_symbols::{CallableDefinitionId, SemanticValueStoreError, TypeId};

/// A violated semantic-input or MIR-construction contract in a compiler-generated body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyntheticLoweringError {
    /// Semantic value construction or lookup failed.
    SemanticValue(SemanticValueStoreError),
    /// A generated MIR identity table exceeded its compact representation.
    Capacity(MirCapacityError),
    /// A selected compiler-provided callable lacks a required result type.
    MissingCallableResult(CallableDefinitionId),
    /// A generated memory operation lacks its required typed result.
    MissingTypeResult(TypeId),
    /// The requested reference does not identify a supported synthetic helper.
    MissingHelper(MirHelperReference),
    /// The checked type has no supported synthetic lowering behavior.
    UnsupportedType(TypeId),
    /// A required closed type or representation is unavailable.
    UnresolvedType(TypeId),
    /// A represented value cannot be processed in the requested lifecycle phase.
    UnsupportedLifecycleRole(MirGeneratedLifecycleRole),
    /// A required compiler-known representation cannot be constructed.
    MissingRepresentation {
        /// Required representation contract.
        role: RepresentationRole,
        /// Generic argument of a unary representation, when applicable.
        argument: Option<TypeId>,
    },
    /// A represented member or element ordinal exceeds the MIR index range.
    LayoutOverflow(TypeId),
}

impl<C: super::SyntheticLoweringContext + ?Sized> super::SyntheticLowerer<'_, C> {
    pub(crate) fn capacity_error(&self, cause: MirCapacityError) -> C::Error {
        SyntheticLoweringError::Capacity(cause).into()
    }
}
