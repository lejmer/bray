//! Lowering from checked semantic trees into Bray MIR.

#![forbid(unsafe_code)]

mod cleanup_await;
mod cleanup_loop;
mod cleanup_outcome;
mod frame_creation;
mod host;
mod identity;
mod input;
mod lowering;
mod operand;
mod plan;
mod raw_buffer;
mod result;
mod specialization;
mod synthetic;

pub use host::{ExecutableHostLoweringInput, ExecutableHostStatic, lower_executable_host};
pub use identity::executable_unit_kind;
pub use input::{LoweringInput, LoweringInputError, LoweringInputKind};
pub use lowering::{LoweringError, lower_unit};
pub use plan::{
    LoweringPlanFailure, LoweringPlanFailureCause, LoweringPlanKind, VerifiedLoweringPlans,
};
pub use result::{CompileTimeUnit, LoweredUnit};
pub use specialization::{
    specialize_destruction_body, specialize_destructor_body, specialize_lifecycle_execution,
};
pub use synthetic::{
    HeapStorageLoweringInput, HeapStorageMethod, SyntheticLoweringContext, SyntheticLoweringError,
    TaskObservationMethod, lower_heap_storage, lower_lifecycle, lower_task_observation,
};
