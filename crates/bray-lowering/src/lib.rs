//! Lowering from checked semantic trees into Bray MIR.

#![forbid(unsafe_code)]

mod cleanup_loop;
mod cleanup_outcome;
mod host;
mod identity;
mod input;
mod lifecycle_call;
mod lowering;
mod operand;
mod plan;
mod result;
mod synthetic;

pub use host::{ExecutableHostLoweringInput, ExecutableHostStatic, lower_executable_host};
pub use identity::executable_unit_kind;
pub use input::{LoweringInput, LoweringInputError, LoweringInputKind};
pub use lowering::{LoweringError, lower_unit};
pub use plan::{
    LoweringPlanFailure, LoweringPlanFailureCause, LoweringPlanKind, VerifiedLoweringPlans,
};
pub use result::{CompileTimeUnit, LoweredUnit};
pub use synthetic::{
    HeapStorageLoweringInput, HeapStorageMethod, SyntheticLoweringContext, SyntheticLoweringError,
    lower_heap_storage, lower_lifecycle,
};
