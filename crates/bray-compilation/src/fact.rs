mod batch;
mod cache;
mod cancellation;
mod cell_map;
mod completion;
mod diagnostic;
mod error;
mod fingerprint;
mod key;
mod priority;
mod runtime;
mod runtime_error;
mod scheduler;
mod symbol;
mod task;
mod unit;
mod unit_identity;

pub use cancellation::CancellationToken;
pub use completion::SymbolCompletionError;
pub use error::ImportedQueryFailure;
pub(crate) use error::diagnostic_context;
pub use error::{FactCycle, FactQueryError, diagnostic_semantic_value_failure};
pub(crate) use error::{
    callable_signature_reason, diagnostic_binding_failure, diagnostic_checker_failure,
    diagnostic_cycle_failure, diagnostic_fact_runtime_failure,
    diagnostic_generic_substitution_failure, diagnostic_semantic_query_failure,
    diagnostic_symbol_graph_failure, generic_substitution_reason,
    push_generic_substitution_failure, push_selected_target_properties,
    push_semantic_value_failure,
};
pub use key::ImportedSemanticRecordKey;
pub use priority::QueryPriority;
pub use runtime_error::{FactRuntimeError, FactRuntimeErrorKind};

pub(crate) use batch::{BatchCompletionError, BatchWork};
pub(crate) use cache::FactCell;
#[cfg(test)]
pub(crate) use cache::{FactCellTestEvent, FactCellTestObserver};
pub(crate) use cancellation::SharedCancellation;
pub(crate) use cell_map::FactCellMap;
pub(crate) use completion::complete_symbol;
pub(crate) use diagnostic::{
    DiagnosticPublicationOrder, OrderedDiagnosticCollection, publish_diagnostics,
};
pub(crate) use fingerprint::{
    CompilationInputKey, CompilationInputs, FactDependencyRecord, FactFingerprint, fact_fingerprint,
};
pub(crate) use key::{
    CodegenArtifactQueryKey, CompilationFactKey, ConstantCallQueryKey, ConstantInstanceQueryKey,
    ImportedExecutableTemplateAddress, IterationSourceQueryKey, NativeProductQueryKey,
    OptimizedMirQueryKey, RuntimeComponentQueryIdentity, SymbolQueryKey,
};
pub(crate) use priority::QueryPriorityDemand;
#[cfg(test)]
pub(crate) use runtime::FactEvaluationTestObserver;
pub(crate) use runtime::{EvaluationCommit, FactRuntime};
pub(crate) use runtime_error::{
    CancellationStateKind, CapacityResource, FactRuntimeFailure, FactTaskPhase, HostIoFailure,
    LocalStateFailure, SchedulerCounter, SchedulerLocalOperation, SynchronizationComponent,
    TaskContextIdentity, TaskLocalOperation, TaskOperation, WorkerPoolKind,
};
pub(crate) use symbol::SymbolQueryCache;
pub(crate) use task::{FactTaskIdentity, RuntimeIdentity};
pub(crate) use unit::{PublishedUnitResult, UnitQueryCache};
pub(crate) use unit_identity::BoundUnitIdentityMap;
