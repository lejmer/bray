//! Backend-independent Bray mid-level intermediate representation.

#![forbid(unsafe_code)]

mod block;
mod control;
mod execution;
mod id;
mod operation;
mod reference;
mod source;
mod storage;
mod target;
#[cfg(test)]
mod test_support;
mod unit;
mod value;
mod walk;

pub use block::{MirBlock, MirBlockKind};
pub use control::{
    MirCleanupEdge, MirCleanupPhase, MirEdge, MirRunResultEdges, MirSwitchCase, MirTerminator,
    MirTerminatorKind,
};
pub use execution::{
    MirFrameDescriptor, MirFrameDescriptorBuildError, MirFrameStateFacts, MirUnitExecution,
};
pub use id::{MirBlockId, MirFrameStateId, MirOperationId, MirStorageId, MirUnitId, MirValueId};
pub use operation::{
    MirBinaryOperator, MirExecutionOperation, MirOperation, MirOperationCommit, MirOperationKind,
    MirTaskTerminalState, MirUnaryOperator,
};
pub use reference::{
    MirCall, MirCallTarget, MirCallableReference, MirFieldReference, MirRuntimeReference,
};
pub use source::{MirSourceAnchor, MirSourceOrigin};
pub use storage::{MirPlace, MirProjection, MirStorage, MirStorageKind};
pub use target::MirTargetFacts;
pub use unit::{MirUnit, MirUnitBuildError, MirUnitBuilder, MirUnitKey};
pub use value::{MirOperand, MirValue, MirValueOrigin};
pub use walk::{MirVisitControl, MirVisitor, walk_mir_unit};
