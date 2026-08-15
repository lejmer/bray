mod cache;
mod cancellation;
mod cell_map;
mod completion;
mod error;
mod fingerprint;
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
pub(crate) use fingerprint::{
    CompilationInputKey, CompilationInputs, FactDependencyRecord, FactFingerprint, fact_fingerprint,
};
pub use key::ImportedSemanticRecordKey;
pub use priority::QueryPriority;

pub(crate) use cache::FactCell;
#[cfg(test)]
pub(crate) use cache::{FactCellTestEvent, FactCellTestObserver};
pub(crate) use cancellation::SharedCancellation;
pub(crate) use cell_map::FactCellMap;
pub(crate) use key::{
    CodegenArtifactFactKey, CompilationFactKey, ConstantCallFactKey, ConstantInstanceFactKey,
    ImportedExecutableTemplateAddress, IterationSourceFactKey, NativeProductFactKey,
    OperationSelectionFactKey, RuntimeComponentFactIdentity, SymbolFactKey,
};
pub(crate) use priority::QueryPriorityDemand;
#[cfg(test)]
pub(crate) use runtime::FactEvaluationTestObserver;
pub(crate) use runtime::{EvaluationCommit, FactRuntime};
pub(crate) use symbol::SymbolFactCache;
pub(crate) use task::FactTaskIdentity;
pub(crate) use unit::{PublishedUnitFact, UnitFactCache};
pub(crate) use unit_identity::BoundUnitIdentityMap;
