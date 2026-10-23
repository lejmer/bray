//! Lowering from checked semantic trees into Bray MIR.

#![forbid(unsafe_code)]

mod cleanup_loop;
mod cleanup_outcome;
mod continuation;
mod host;
mod identity;
mod input;
mod lifecycle_call;
mod lowering;
mod operand;
mod predicate;
mod result;
mod synthetic;

pub use host::{ExecutableHostLoweringInput, ExecutableHostStatic, lower_executable_host};
pub use identity::executable_unit_kind;
pub use input::{DeclarationPreconditions, LoweringInput};
pub use lowering::{LoweringError, lower_execution_requirements, lower_unit};
pub use predicate::{PredicateLoweringContext, lower_predicate};
pub use result::{CompileTimeUnit, LoweredUnit};
pub use synthetic::{
    DestructionBody, HeapStorageLoweringInput, HeapStorageMethod, SyntheticLoweringContext,
    SyntheticLoweringError, expand_lifecycle_continuations, lower_heap_storage, lower_lifecycle,
};
