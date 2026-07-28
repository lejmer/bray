use bray_runtime_interface::RuntimeAbiRole;

use crate::{MirBlockId, MirCleanupPhase, MirOperationId, MirStorageId, MirUnitId, MirValueId};

/// A contract violation that prevents creation of a MIR unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MirUnitBuildError {
    /// An element's source provenance does not belong to the MIR unit.
    SourceOriginMismatch,
    /// A unit-local identity table exceeded its compact representation.
    IdentityCapacityExceeded,
    /// A referenced block belongs to another MIR unit.
    ForeignBlock {
        /// Unit being constructed.
        expected: MirUnitId,
        /// Unit carried by the rejected block ID.
        actual: MirUnitId,
    },
    /// A referenced operation belongs to another MIR unit.
    ForeignOperation(MirOperationId),
    /// A referenced storage belongs to another MIR unit.
    ForeignStorage(MirStorageId),
    /// A referenced value belongs to another MIR unit.
    ForeignValue(MirValueId),
    /// A block ID does not name a committed block.
    MissingBlock(MirBlockId),
    /// An operation ID does not name a committed operation.
    MissingOperation(MirOperationId),
    /// A value-producing operation has no result value.
    MissingOperationResult(MirOperationId),
    /// An operation that writes through storage also declares a result value.
    UnexpectedOperationResult(MirOperationId),
    /// An operation result has a type inconsistent with its operation payload.
    OperationResultTypeMismatch(MirOperationId),
    /// An aggregate operation has an invalid operand shape.
    InvalidAggregateOperation(MirOperationId),
    /// A construction input does not belong to its target or repeats another input.
    InvalidConstructionInput(MirOperationId),
    /// A storage ID does not name a committed storage allocation.
    MissingStorage(MirStorageId),
    /// A value ID does not name a committed value.
    MissingValue(MirValueId),
    /// A block already has a terminator.
    DuplicateTerminator(MirBlockId),
    /// A block has no terminator.
    MissingTerminator(MirBlockId),
    /// An edge supplies the wrong number of destination arguments.
    EdgeArgumentCountMismatch(MirBlockId),
    /// An edge argument type does not match its destination parameter.
    EdgeArgumentTypeMismatch(MirBlockId),
    /// A switch contains the same canonical case value more than once.
    DuplicateSwitchCase(MirBlockId),
    /// A cleanup edge targets a block with the wrong cleanup role.
    CleanupTargetMismatch {
        /// Declared cleanup phase.
        phase: MirCleanupPhase,
        /// Rejected target block.
        target: MirBlockId,
    },
    /// A phase-one cleanup block does not continue into phase two.
    CleanupPhaseOrderViolation(MirBlockId),
    /// A private runtime operation carries the wrong closed ABI role.
    RuntimeRoleMismatch {
        /// Role required by the operation.
        expected: RuntimeAbiRole,
        /// Role supplied by lowering.
        actual: RuntimeAbiRole,
    },
    /// A private runtime reference uses another selected ABI version.
    RuntimeAbiVersionMismatch,
    /// An operation is not legal in its containing block role.
    InvalidOperationBlock(MirOperationId),
    /// An operation references storage with the wrong semantic role.
    StorageKindMismatch(MirStorageId),
    /// A typed storage operation has incompatible input and destination types.
    StorageTypeMismatch(MirStorageId),
    /// A value is used outside the control-flow region where it is defined.
    ValueDoesNotDominateUse(MirValueId),
    /// A child task does not have one start and one terminal destruction.
    InvalidTaskLifecycle(MirStorageId),
    /// A frame operation references a frame other than the unit's protected frame.
    ProtectedFrameMismatch,
    /// A protected-frame unit has no matching hidden frame descriptor.
    MissingFrameDescriptor,
    /// A protected-frame unit already has a hidden frame descriptor.
    DuplicateFrameDescriptor,
    /// A synchronous or executable-host unit carries a protected-frame descriptor.
    UnexpectedFrameDescriptor,
    /// A protected-frame state references a missing or foreign entry block.
    InvalidFrameStateEntry(MirBlockId),
    /// A protected-frame operation references a state absent from its descriptor.
    MissingFrameState,
}
