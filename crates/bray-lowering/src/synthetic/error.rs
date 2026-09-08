use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
use bray_ir::{
    MirGeneratedLifecycleRole, MirHelperReference, MirOperationId, MirSourceAnchor,
    MirUnitBuildError,
};
use bray_symbols::{CallableDefinitionId, SemanticValueStoreError, TypeData, TypeId};

/// A violated semantic-input or MIR-construction contract in a compiler-generated body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyntheticLoweringError {
    /// Semantic value construction or lookup failed.
    SemanticValue(SemanticValueStoreError),
    /// The MIR builder rejected a generated lifecycle body's operation or control flow.
    LifecycleMir {
        /// Source or generated owner of the invalid body.
        owner: bray_ir::MirSourceOrigin,
        /// Exact violated MIR invariant.
        cause: MirUnitBuildError,
    },
    /// A concrete lifecycle expansion violated the template's MIR contract.
    SpecializedMir {
        /// Original source or imported template location.
        source: MirSourceAnchor,
        /// Exact violated MIR invariant.
        cause: MirUnitBuildError,
    },
    /// The generated protected-frame state table violates its descriptor contract.
    FrameDescriptor(bray_ir::MirFrameDescriptorBuildError),
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
    /// An internal storage member key is malformed.
    InvalidStorageMemberKey(String),
    /// The requested reference does not identify a supported synthetic helper.
    MissingHelper(MirHelperReference),
    /// The checked type has no supported synthetic lowering behavior.
    UnsupportedType(TypeId),
    /// A required closed type or representation is unavailable.
    UnresolvedType(TypeId),
    /// A represented value cannot be processed in the requested lifecycle phase.
    UnsupportedLifecycleRole(MirGeneratedLifecycleRole),
    /// A value with a borrowed or callable type reached represented-value teardown.
    UnexpectedLifecycleType {
        /// Type requested for teardown.
        ty: TypeId,
        /// Actual semantic type data.
        actual: TypeData,
    },
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
    /// A selected storage protocol method does not have its required single parameter.
    StorageParameterCount {
        /// Exact compiler-known storage member.
        member: CompilerKnownDeclarationKey,
        /// Actual selected parameter count.
        actual: usize,
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
            MirSourceAnchor::GeneratedLifecycle(reference) => {
                SyntheticLoweringError::LifecycleMir {
                    owner: bray_ir::MirSourceOrigin::GeneratedLifecycle(reference.clone()),
                    cause,
                }
                .into()
            }
            MirSourceAnchor::Source(_)
            | MirSourceAnchor::ExecutableHost(_)
            | MirSourceAnchor::ImportedExecutable(_) => SyntheticLoweringError::SpecializedMir {
                source: source.clone(),
                cause,
            }
            .into(),
        }
    }
}
