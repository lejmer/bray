use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockId, BoundExpressionId, BoundOperator, BoundPatternId, BoundUnitRoot,
    StorageAccessId, StorageIdentityId,
};
use bray_compiler_known::RepresentationRole;
use bray_ir::MirUnitBuildError;

/// A violated checked-HIR or MIR construction contract encountered during lowering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    /// Checked async facts omitted a direct-await suspension decision.
    MissingSuspensionPoint(BoundExpressionId),
    /// A checked future or task operation has an incompatible call shape.
    InvalidTaskOperation(BoundExpressionId),
    /// A protected callable frame has no checked completion type.
    MissingCallableResultType,
    /// A literal has no canonical checked value.
    MissingLiteralValue(BoundExpressionId),
    /// A checked expression has no required semantic selection.
    MissingSemanticSelection(BoundExpressionId),
    /// The synchronous lowering core does not yet cover this expression category.
    UnsupportedExpression(BoundExpressionId),
    /// The synchronous lowering core does not yet cover this binding pattern.
    UnsupportedPattern(BoundPatternId),
    /// An operator selected for the synchronous core has no MIR operation.
    UnsupportedOperator(BoundOperator),
    /// An expression has no checked storage access.
    MissingStorageAccess(BoundExpressionId),
    /// A checked storage access is absent from the canonical plan.
    MissingStorageAccessRecord(StorageAccessId),
    /// Checked storage flow names a lexical exit without a corresponding cleanup plan.
    MissingCleanupPlan(BoundBlockId),
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
    /// Checked async facts could not form one coherent frame descriptor.
    InvalidFrameDescriptor,
    /// The MIR builder or validator rejected the lowered unit.
    Mir(MirUnitBuildError),
}

impl From<MirUnitBuildError> for LoweringError {
    fn from(error: MirUnitBuildError) -> Self {
        Self::Mir(error)
    }
}
