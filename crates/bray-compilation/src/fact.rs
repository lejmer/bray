mod cache;
mod cancellation;
mod cell_map;
mod completion;
mod error;
mod key;
mod priority;
mod runtime;
mod scheduler;
mod symbol;
mod task;
mod unit;
mod unit_identity;

pub use cancellation::CancellationToken;
pub use completion::SymbolCompletionError;

pub(crate) use completion::force_complete_symbol;
pub use error::{FactCycle, FactQueryError};
pub use key::ImportedSemanticFactKey;
pub use priority::QueryPriority;

pub(crate) use cache::FactCell;
#[cfg(test)]
pub(crate) use cache::{FactCellTestEvent, FactCellTestObserver};
pub(crate) use cancellation::SharedCancellation;
pub(crate) use cell_map::FactCellMap;
pub(crate) use key::{
    CompilationFactKey, ConstantCallFactKey, ConstantInstanceFactKey, IterationSourceFactKey,
    SymbolFactKey,
};
pub(crate) use priority::QueryPriorityDemand;
pub(crate) use runtime::{EvaluationCommit, FactRuntime};
pub(crate) use symbol::SymbolFactCache;
pub(crate) use task::FactTaskIdentity;
pub(crate) use unit::{PublishedUnitFact, UnitFactCache};
pub(crate) use unit_identity::BoundUnitIdentityMap;
