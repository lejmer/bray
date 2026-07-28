use bray_bound_tree::{
    AnyBoundNodeId, BoundExpressionId, BoundOperator, BoundPatternId, BoundUnitRoot,
    StorageAccessId, StorageIdentityId,
};
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
    /// A call depends on a declaration-owned default that must be lowered separately.
    UnsupportedDefaultArgument(BoundExpressionId),
    /// An expression has no checked storage access.
    MissingStorageAccess(BoundExpressionId),
    /// A checked storage access is absent from the canonical plan.
    MissingStorageAccessRecord(StorageAccessId),
    /// A checked access reaches no persistent storage identity.
    MissingStorageIdentity(StorageAccessId),
    /// A persistent storage identity is absent from the canonical plan.
    MissingStorageIdentityRecord(StorageIdentityId),
    /// The synchronous core does not yet cover this storage path.
    UnsupportedStorageAccess(StorageAccessId),
    /// A value-producing MIR operation did not publish its required result.
    MissingOperationResult(BoundExpressionId),
    /// The MIR builder or validator rejected the lowered unit.
    Mir(MirUnitBuildError),
}

impl From<MirUnitBuildError> for LoweringError {
    fn from(error: MirUnitBuildError) -> Self {
        Self::Mir(error)
    }
}
