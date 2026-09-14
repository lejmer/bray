use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirGeneratedLifecycleRole, MirHelperReference, MirOperationId, MirSourceAnchor,
    MirUnitBuildError,
};
use bray_symbols::{CallableDefinitionId, SemanticValueStoreError, TypeId};

/// A violated semantic-input or MIR-construction contract in a compiler-generated body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyntheticLoweringError {
    /// Semantic value construction or lookup failed.
    SemanticValue(SemanticValueStoreError),
    /// The MIR builder rejected a generated lifecycle body's operation or control flow.
    LifecycleMir(MirUnitBuildError),
    /// The MIR builder rejected an exact compiler-provided callable body.
    CompilerProvidedMir {
        /// Exact declaration whose body was being lowered.
        definition: CallableDefinitionId,
        /// Builder or validation failure.
        cause: MirUnitBuildError,
    },
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
    /// A generated value-producing operation did not publish a result.
    MissingOperationResult {
        /// Exact generated body owner.
        source: MirSourceAnchor,
        /// Operation whose result is missing.
        operation: MirOperationId,
    },
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
    pub(crate) fn mir_error(&self, source: &MirSourceAnchor, cause: MirUnitBuildError) -> C::Error {
        match source {
            MirSourceAnchor::CompilerProvidedCallable(definition) => {
                SyntheticLoweringError::CompilerProvidedMir {
                    definition: *definition,
                    cause,
                }
                .into()
            }
            // Lifecycle and Heap lowering are the only callers of this private constructor.
            MirSourceAnchor::GeneratedLifecycle(_) => {
                SyntheticLoweringError::LifecycleMir(cause).into()
            }
            MirSourceAnchor::Source(_)
            | MirSourceAnchor::ExecutableHost(_)
            | MirSourceAnchor::ImportedExecutable(_) => {
                unreachable!("synthetic lowering owns only lifecycle and compiler-provided bodies")
            }
        }
    }
}
