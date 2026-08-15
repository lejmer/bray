//! Backend-independent Bray mid-level intermediate representation.

#![forbid(unsafe_code)]

mod block;
mod control;
mod frame;
mod helper;
mod id;
mod operation;
mod reference;
mod source;
mod storage;
mod target;
#[cfg(test)]
mod test_support;
mod types;
mod unit;
mod value;
mod walk;

pub use block::{MirBlock, MirBlockKind};
pub use bray_bound_tree::{
    BoundUnitKey, ConstructionInputId, ConstructionTarget, ConversionTarget, PatternOperation,
    PatternPredicate, PatternProjection, SelectedConversion,
};
pub use control::{
    MirCleanupEdge, MirCleanupPhase, MirEdge, MirPatternPredicate, MirRunResultEdges,
    MirInlineAssemblyTerminator, MirSuspensionKind, MirSwitchCase, MirTerminator,
    MirTerminatorKind,
};
pub use frame::{
    MirFrameDescriptor, MirFrameDescriptorBuildError, MirFrameReference, MirFrameState,
};
pub use helper::MirHelperReference;
pub use id::{MirBlockId, MirFrameStateId, MirOperationId, MirStorageId, MirUnitId, MirValueId};
pub use operation::{
    MirAggregate, MirAggregateKind, MirAsyncOperation, MirBinaryOperator, MirConstruction,
    MirConstructionInput, MirFrameInitializer, MirGeneratorKind, MirGeneratorOperation,
    MirHostOperation, MirMemoryOperation, MirNumericConversionKind, MirOperation,
    MirOperationCommit, MirOperationKind, MirPanicCause, MirStoreKind, MirTaskTerminalState,
    MirTextOperation, MirTextOperationKind, MirUnaryOperator,
};
pub use reference::{
    MirAnonymousCallableReference, MirCall, MirCallArgument, MirCallIntrinsic, MirCallTarget,
    MirCallableReference, MirFieldReference, MirRuntimeReference,
};
pub use source::{MirSourceAnchor, MirSourceOrigin};
pub use storage::{MirPlace, MirProjection, MirProjectionKind, MirStorage, MirStorageKind};
pub use target::MirTargetContract;
pub use unit::{
    MirExecutableTemplateId, MirGeneratedLifecycleKey, MirGeneratedLifecycleRole,
    MirImportedExecutableKey, MirUnit, MirUnitBuildError, MirUnitBuilder, MirUnitKey, MirUnitKind,
};
pub use value::{MirImmediateValue, MirOperand, MirValue, MirValueOrigin};
pub use walk::{MirVisitControl, MirVisitor, walk_mir_unit};
