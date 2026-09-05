use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockId, BoundExpressionId, BoundOperator, BoundPatternId, BoundUnitRoot,
    StorageAccessId, StorageIdentityId,
};
use bray_compiler_known::RepresentationRole;
use bray_ir::{MirFrameDescriptorBuildError, MirUnitBuildError};
use bray_symbols::{GenericSubstitutionShapeError, SemanticValueStoreError};

/// A violated checked-HIR or MIR construction contract encountered during lowering.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LoweringError {
    /// The bound unit root is not part of the synchronous lowering contract.
    UnsupportedRoot(BoundUnitRoot),
    /// A required bound node is absent from the immutable unit.
    MissingBoundNode(AnyBoundNodeId),
    /// Recovery reached production lowering.
    RecoveredBoundNode(AnyBoundNodeId),
    /// A checked expression has no final type.
    MissingExpressionType(BoundExpressionId),
    /// Direct await reached lowering without a protected current frame.
    AwaitOutsideProtectedFrame(BoundExpressionId),
    /// Checked async analysis omitted a direct-await suspension decision.
    MissingSuspensionPoint(BoundExpressionId),
    /// A checked future or task operation has an incompatible call shape.
    InvalidTaskOperation(BoundExpressionId),
    /// A protected callable frame has no checked completion type.
    MissingCallableResultType,
    /// Active lowering scopes do not contain the requested cleanup depth.
    InvalidCleanupScopeDepth {
        /// First active scope that must be cleaned.
        scope_depth: usize,
        /// Number of scopes active at the exit.
        active_scope_count: usize,
        /// Exact bound occurrence initiating cleanup.
        exit: AnyBoundNodeId,
    },
    /// The verified plan set has no decision for one active scope and exit.
    MissingScopeExitPlan {
        /// Active lexical scope whose decision is absent.
        scope: BoundBlockId,
        /// Exact bound occurrence initiating cleanup.
        exit: AnyBoundNodeId,
    },
    /// A literal has no canonical checked value.
    MissingLiteralValue(BoundExpressionId),
    /// A checked expression has no required semantic selection.
    MissingSemanticSelection(BoundExpressionId),
    /// The synchronous lowering core does not yet cover this expression category.
    UnsupportedExpression(BoundExpressionId),
    /// The synchronous lowering core does not yet cover this binding pattern.
    UnsupportedPattern(BoundPatternId),
    /// An operator selected for the synchronous core has no MIR operation.
    UnsupportedOperator {
        expression: BoundExpressionId,
        operator: BoundOperator,
    },
    /// An expression has no checked storage access.
    MissingStorageAccess(BoundExpressionId),
    /// A checked storage access is absent from the canonical plan.
    MissingStorageAccessRecord(StorageAccessId),
    /// A checked access reaches no persistent storage identity.
    MissingStorageIdentity(StorageAccessId),
    /// A persistent storage identity is absent from the canonical plan.
    MissingStorageIdentityRecord(StorageIdentityId),
    /// An iteration expression has no checked cursor or element storage.
    MissingIterationStorage(BoundExpressionId),
    /// The synchronous core does not yet cover this storage path.
    UnsupportedStorageAccess(StorageAccessId),
    /// A value-producing MIR operation did not publish its required result.
    MissingOperationResult(BoundExpressionId),
    /// A required compiler-known representation is unavailable for the selected target.
    MissingRepresentation(RepresentationRole),
    /// A checked semantic value could not be read or interned.
    SemanticValueUnavailable,
    /// Generic substitution construction rejected an exact parameter-to-argument relationship.
    GenericSubstitution(GenericSubstitutionShapeError),
    /// The semantic value store rejected a required read or intern operation.
    SemanticValue(SemanticValueStoreError),
    /// Checked async analysis could not form one coherent frame descriptor.
    InvalidFrameDescriptor(MirFrameDescriptorBuildError),
    /// A selected memory argument ordinal cannot index the host collection.
    MemoryArgumentOrdinalUnrepresentable {
        /// Memory operation whose argument ordinal was rejected.
        expression: BoundExpressionId,
        /// Exact selected ordinal.
        ordinal: u32,
    },
    /// A match-arm ordinal cannot be represented by the MIR protocol.
    MatchArmOrdinalUnrepresentable {
        /// Match expression containing the arm.
        expression: BoundExpressionId,
        /// Exact zero-based arm ordinal.
        ordinal: usize,
    },
    /// The MIR builder or validator rejected the lowered unit.
    Mir(MirUnitBuildError),
}

impl From<MirUnitBuildError> for LoweringError {
    fn from(error: MirUnitBuildError) -> Self {
        Self::Mir(error)
    }
}

impl From<SemanticValueStoreError> for LoweringError {
    fn from(error: SemanticValueStoreError) -> Self {
        Self::SemanticValue(error)
    }
}

impl From<crate::plan::CleanupPlanLookupError> for LoweringError {
    fn from(error: crate::plan::CleanupPlanLookupError) -> Self {
        match error {
            crate::plan::CleanupPlanLookupError::InvalidScopeDepth {
                scope_depth,
                active_scope_count,
                exit,
            } => Self::InvalidCleanupScopeDepth {
                scope_depth,
                active_scope_count,
                exit,
            },
            crate::plan::CleanupPlanLookupError::MissingScopeExit { scope, exit } => {
                Self::MissingScopeExitPlan { scope, exit }
            }
        }
    }
}
