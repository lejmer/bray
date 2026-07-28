//! Backend-independent Bray mid-level intermediate representation.

#![forbid(unsafe_code)]

mod block;
mod control;
mod frame;
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
pub use frame::{MirFrameDescriptor, MirFrameDescriptorBuildError, MirFrameStateFacts};
pub use id::{MirBlockId, MirFrameStateId, MirOperationId, MirStorageId, MirUnitId, MirValueId};
pub use operation::{
    MirAggregate, MirAggregateKind, MirAsyncOperation, MirBinaryOperator, MirConstruction,
    MirConstructionInput, MirGeneratorKind, MirGeneratorOperation, MirOperation,
    MirOperationCommit, MirOperationKind, MirPanicCause, MirStoreKind, MirTaskTerminalState,
    MirUnaryOperator,
};
pub use reference::{
    MirCall, MirCallArgument, MirCallTarget, MirCallableReference, MirFieldReference,
    MirRuntimeReference,
};
pub use source::{MirSourceAnchor, MirSourceOrigin};
pub use storage::{MirPlace, MirProjection, MirProjectionKind, MirStorage, MirStorageKind};
pub use target::MirTargetFacts;
pub use unit::{MirUnit, MirUnitBuildError, MirUnitBuilder, MirUnitKey, MirUnitKind};
pub use value::{MirImmediateValue, MirOperand, MirValue, MirValueOrigin};
pub use walk::{MirVisitControl, MirVisitor, walk_mir_unit};
