use std::io;

use bray_bound_tree::{BoundUnitId, BoundUnitKey};

use super::{CompilationFactKey, FactTaskIdentity, RuntimeIdentity};

/// Stable category for a compiler query runtime failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FactRuntimeErrorKind {
    /// Shared runtime state was poisoned by an unexpected panic.
    SynchronizationPoisoned,
    /// A bounded runtime identity or counter could not represent another value.
    CapacityExhausted,
    /// A compiler query worker pool could not be created.
    WorkerPoolCreation,
    /// A compiler query worker terminated before publishing its result.
    WorkerTerminated,
    /// A task operation was incompatible with the task's retained phase.
    InvalidTaskState,
    /// Scheduler counters, registrations, or worker-local state violated their contract.
    InvalidSchedulerState,
    /// Required worker-local task context was unavailable or belonged to another runtime.
    InvalidTaskContext,
    /// A task no longer retained the computation needed to publish its fact.
    AbandonedComputation,
    /// Retained dependency or wait-graph state was incomplete.
    DependencyStateMismatch,
    /// A cancellation interest violated shared evaluation state.
    InvalidCancellationState,
    /// A fact operation was applied to an incompatible fact category.
    InvalidFactState,
    /// Bound-unit identity construction violated its deterministic identity contract.
    UnitIdentityFailure,
}

/// Exact failure retained by the compiler's demand-driven query runtime.
///
/// The stable category is public while compilation-local keys, task identities, and fingerprints
/// remain private to the compilation that owns them.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FactRuntimeError {
    // Query errors sit on recursive compiler stacks, so private payload growth stays indirect.
    cause: Box<FactRuntimeFailure>,
}

impl FactRuntimeError {
    /// Returns the stable runtime failure category.
    pub fn kind(&self) -> FactRuntimeErrorKind {
        self.cause().kind()
    }

    pub(crate) fn cause(&self) -> &FactRuntimeFailure {
        &self.cause
    }
}

impl std::fmt::Display for FactRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "compiler query runtime failure: {:?}",
            self.cause()
        )
    }
}

impl std::error::Error for FactRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.cause() {
            FactRuntimeFailure::WorkerPoolCreation {
                host: Some(host), ..
            } => Some(host),
            _ => None,
        }
    }
}

impl From<FactRuntimeFailure> for FactRuntimeError {
    fn from(cause: FactRuntimeFailure) -> Self {
        Self {
            cause: Box::new(cause),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum FactRuntimeFailure {
    SynchronizationPoisoned {
        component: SynchronizationComponent,
        fact: Option<CompilationFactKey>,
        task: Option<FactTaskIdentity>,
    },
    SchedulerResultStatePoisoned {
        item: Option<usize>,
    },
    UnitIdentityStatePoisoned {
        unit: BoundUnitKey,
    },
    CapacityExhausted {
        resource: CapacityResource,
        fact: Option<CompilationFactKey>,
        task: Option<FactTaskIdentity>,
    },
    WorkerPoolCreation {
        pool: WorkerPoolKind,
        workers: usize,
        host: Option<HostIoFailure>,
    },
    WorkerTerminated {
        worker: Option<usize>,
        item: Option<usize>,
    },
    InvalidTaskState {
        operation: TaskOperation,
        expected: FactTaskPhase,
        actual: FactTaskPhase,
        task: FactTaskIdentity,
        fact: CompilationFactKey,
    },
    InvalidSchedulerState {
        counter: SchedulerCounter,
        expected_minimum: usize,
        actual: usize,
    },
    InvalidTaskContext {
        expected_runtime: RuntimeIdentity,
        actual: TaskContextIdentity,
    },
    TaskLocalStateUnavailable {
        operation: TaskLocalOperation,
        cause: LocalStateFailure,
    },
    SchedulerLocalStateUnavailable {
        operation: SchedulerLocalOperation,
        cause: LocalStateFailure,
    },
    AbandonedComputation {
        task: FactTaskIdentity,
        fact: CompilationFactKey,
    },
    InvalidWaitGraph {
        requester: FactTaskIdentity,
        owner: FactTaskIdentity,
        missing_predecessor: FactTaskIdentity,
        requested: CompilationFactKey,
    },
    MissingCycle {
        runtime: RuntimeIdentity,
        fact: CompilationFactKey,
        active: Box<[CompilationFactKey]>,
    },
    InvalidCancellationState {
        expected: CancellationStateKind,
        actual: CancellationStateKind,
    },
    RecursiveCancellationInterest,
    InvalidFrozenFact {
        fact: CompilationFactKey,
    },
    InvalidUnitQueryKey {
        fact: CompilationFactKey,
        unit: BoundUnitKey,
    },
    UnitSourceCapacityExhausted {
        source_count: usize,
    },
    UnknownUnitSource {
        unit: BoundUnitKey,
    },
    UnitIdentityCapacityExhausted {
        unit: BoundUnitKey,
        source_ordinal: u32,
    },
    UnitIdentityCollision {
        identity: BoundUnitId,
        expected: BoundUnitKey,
        actual: BoundUnitKey,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct HostIoFailure {
    kind: io::ErrorKind,
    raw_os_error: Option<i32>,
    message: Box<str>,
}

impl From<&io::Error> for HostIoFailure {
    fn from(error: &io::Error) -> Self {
        Self {
            kind: error.kind(),
            raw_os_error: error.raw_os_error(),
            message: error.to_string().into_boxed_str(),
        }
    }
}

impl HostIoFailure {
    pub(crate) const fn kind(&self) -> io::ErrorKind {
        self.kind
    }

    pub(crate) const fn raw_os_error(&self) -> Option<i32> {
        self.raw_os_error
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for HostIoFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for HostIoFailure {}

impl FactRuntimeFailure {
    const fn kind(&self) -> FactRuntimeErrorKind {
        match self {
            Self::SynchronizationPoisoned { .. }
            | Self::SchedulerResultStatePoisoned { .. }
            | Self::UnitIdentityStatePoisoned { .. } => {
                FactRuntimeErrorKind::SynchronizationPoisoned
            }
            Self::CapacityExhausted { .. }
            | Self::UnitSourceCapacityExhausted { .. }
            | Self::UnitIdentityCapacityExhausted { .. } => FactRuntimeErrorKind::CapacityExhausted,
            Self::WorkerPoolCreation { .. } => FactRuntimeErrorKind::WorkerPoolCreation,
            Self::WorkerTerminated { .. } => FactRuntimeErrorKind::WorkerTerminated,
            Self::InvalidTaskState { .. } => FactRuntimeErrorKind::InvalidTaskState,
            Self::InvalidSchedulerState { .. } | Self::SchedulerLocalStateUnavailable { .. } => {
                FactRuntimeErrorKind::InvalidSchedulerState
            }
            Self::InvalidTaskContext { .. } | Self::TaskLocalStateUnavailable { .. } => {
                FactRuntimeErrorKind::InvalidTaskContext
            }
            Self::AbandonedComputation { .. } => FactRuntimeErrorKind::AbandonedComputation,
            Self::InvalidWaitGraph { .. } | Self::MissingCycle { .. } => {
                FactRuntimeErrorKind::DependencyStateMismatch
            }
            Self::InvalidCancellationState { .. } | Self::RecursiveCancellationInterest => {
                FactRuntimeErrorKind::InvalidCancellationState
            }
            Self::InvalidFrozenFact { .. } => FactRuntimeErrorKind::InvalidFactState,
            Self::InvalidUnitQueryKey { .. }
            | Self::UnknownUnitSource { .. }
            | Self::UnitIdentityCollision { .. } => FactRuntimeErrorKind::UnitIdentityFailure,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SynchronizationComponent {
    CancellationInterests,
    CellMap,
    EmbeddedConstantExpectations,
    FactCell,
    RuntimeDependencies,
    SchedulerSlots,
    TaskDependencies,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum CapacityResource {
    CancellationInterestIdentity,
    CellMapAccessIdentity,
    SchedulerInteractiveStreak,
    SchedulerInteractiveWaiters,
    SchedulerOrdinaryWaiters,
    SchedulerWaitRegistrations,
    SchedulerActiveSlots,
    TaskIdentity,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum WorkerPoolKind {
    Interactive,
    Ordinary,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum FactTaskPhase {
    Recording,
    Finished,
    Discarded,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum TaskOperation {
    Finish,
    RecordFact,
    RecordFixedInput,
    RecordFrozenFact,
    RecordInput,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct TaskContextIdentity {
    pub(crate) runtime: RuntimeIdentity,
    pub(crate) task: FactTaskIdentity,
    pub(crate) fact: CompilationFactKey,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum TaskLocalOperation {
    Capture,
    Current,
    Cycle,
    Enter,
    Replace,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SchedulerLocalOperation {
    CurrentPriority,
    Enter,
    Inspect,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SchedulerCounter {
    InteractiveWaiters,
    OrdinaryWaiters,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum LocalStateFailure {
    BorrowConflict,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum CancellationStateKind {
    Request,
    Shared,
}

#[cfg(test)]
mod tests {
    use super::{
        CancellationStateKind, CapacityResource, FactRuntimeError, FactRuntimeErrorKind,
        FactRuntimeFailure, FactTaskPhase, HostIoFailure, LocalStateFailure, SchedulerCounter,
        SchedulerLocalOperation, SynchronizationComponent, TaskContextIdentity, TaskLocalOperation,
        TaskOperation, WorkerPoolKind,
    };
    use crate::fact::{CompilationFactKey, FactTaskIdentity, RuntimeIdentity};
    use crate::test_support::callable_body_key;

    #[test]
    fn opaque_runtime_errors_keep_query_stack_frames_pointer_sized() {
        assert_eq!(
            std::mem::size_of::<FactRuntimeError>(),
            std::mem::size_of::<usize>()
        );
    }

    #[test]
    fn public_kinds_project_every_private_cause_category() {
        let task = FactTaskIdentity(1);
        let fact = CompilationFactKey::SyntaxTree;
        let unit = callable_body_key(0);

        let failures = [
            (
                FactRuntimeFailure::SynchronizationPoisoned {
                    component: SynchronizationComponent::FactCell,
                    fact: Some(fact.clone()),
                    task: Some(task),
                },
                FactRuntimeErrorKind::SynchronizationPoisoned,
            ),
            (
                FactRuntimeFailure::SchedulerResultStatePoisoned { item: Some(0) },
                FactRuntimeErrorKind::SynchronizationPoisoned,
            ),
            (
                FactRuntimeFailure::UnitIdentityStatePoisoned { unit: unit.clone() },
                FactRuntimeErrorKind::SynchronizationPoisoned,
            ),
            (
                FactRuntimeFailure::CapacityExhausted {
                    resource: CapacityResource::TaskIdentity,
                    fact: Some(fact.clone()),
                    task: Some(task),
                },
                FactRuntimeErrorKind::CapacityExhausted,
            ),
            (
                FactRuntimeFailure::UnitSourceCapacityExhausted { source_count: 1 },
                FactRuntimeErrorKind::CapacityExhausted,
            ),
            (
                FactRuntimeFailure::UnitIdentityCapacityExhausted {
                    unit: unit.clone(),
                    source_ordinal: 1,
                },
                FactRuntimeErrorKind::CapacityExhausted,
            ),
            (
                FactRuntimeFailure::WorkerPoolCreation {
                    pool: WorkerPoolKind::Ordinary,
                    workers: 2,
                    host: None,
                },
                FactRuntimeErrorKind::WorkerPoolCreation,
            ),
            (
                FactRuntimeFailure::WorkerTerminated {
                    worker: Some(2),
                    item: Some(9),
                },
                FactRuntimeErrorKind::WorkerTerminated,
            ),
            (
                FactRuntimeFailure::InvalidTaskState {
                    operation: TaskOperation::Finish,
                    expected: FactTaskPhase::Recording,
                    actual: FactTaskPhase::Finished,
                    task,
                    fact: fact.clone(),
                },
                FactRuntimeErrorKind::InvalidTaskState,
            ),
            (
                FactRuntimeFailure::InvalidSchedulerState {
                    counter: SchedulerCounter::OrdinaryWaiters,
                    expected_minimum: 1,
                    actual: 0,
                },
                FactRuntimeErrorKind::InvalidSchedulerState,
            ),
            (
                FactRuntimeFailure::SchedulerLocalStateUnavailable {
                    operation: SchedulerLocalOperation::Enter,
                    cause: LocalStateFailure::BorrowConflict,
                },
                FactRuntimeErrorKind::InvalidSchedulerState,
            ),
            (
                FactRuntimeFailure::InvalidTaskContext {
                    expected_runtime: RuntimeIdentity(1),
                    actual: TaskContextIdentity {
                        runtime: RuntimeIdentity(2),
                        task,
                        fact: fact.clone(),
                    },
                },
                FactRuntimeErrorKind::InvalidTaskContext,
            ),
            (
                FactRuntimeFailure::TaskLocalStateUnavailable {
                    operation: TaskLocalOperation::Capture,
                    cause: LocalStateFailure::Unavailable,
                },
                FactRuntimeErrorKind::InvalidTaskContext,
            ),
            (
                FactRuntimeFailure::AbandonedComputation {
                    task,
                    fact: fact.clone(),
                },
                FactRuntimeErrorKind::AbandonedComputation,
            ),
            (
                FactRuntimeFailure::InvalidWaitGraph {
                    requester: task,
                    owner: FactTaskIdentity(2),
                    missing_predecessor: FactTaskIdentity(3),
                    requested: fact.clone(),
                },
                FactRuntimeErrorKind::DependencyStateMismatch,
            ),
            (
                FactRuntimeFailure::MissingCycle {
                    runtime: RuntimeIdentity(1),
                    fact: fact.clone(),
                    active: vec![fact.clone()].into_boxed_slice(),
                },
                FactRuntimeErrorKind::DependencyStateMismatch,
            ),
            (
                FactRuntimeFailure::InvalidCancellationState {
                    expected: CancellationStateKind::Shared,
                    actual: CancellationStateKind::Request,
                },
                FactRuntimeErrorKind::InvalidCancellationState,
            ),
            (
                FactRuntimeFailure::RecursiveCancellationInterest,
                FactRuntimeErrorKind::InvalidCancellationState,
            ),
            (
                FactRuntimeFailure::InvalidFrozenFact { fact: fact.clone() },
                FactRuntimeErrorKind::InvalidFactState,
            ),
            (
                FactRuntimeFailure::InvalidUnitQueryKey {
                    fact,
                    unit: unit.clone(),
                },
                FactRuntimeErrorKind::UnitIdentityFailure,
            ),
            (
                FactRuntimeFailure::UnknownUnitSource { unit: unit.clone() },
                FactRuntimeErrorKind::UnitIdentityFailure,
            ),
            (
                FactRuntimeFailure::UnitIdentityCollision {
                    identity: bray_bound_tree::BoundUnitId::new(0),
                    expected: unit.clone(),
                    actual: unit,
                },
                FactRuntimeErrorKind::UnitIdentityFailure,
            ),
        ];

        for (failure, expected) in failures {
            assert_eq!(FactRuntimeError::from(failure).kind(), expected);
        }
    }

    #[test]
    fn worker_pool_creation_exposes_the_owned_host_cause() {
        let host = std::io::Error::from_raw_os_error(5);
        let expected = host.to_string();

        let error = FactRuntimeError::from(FactRuntimeFailure::WorkerPoolCreation {
            pool: WorkerPoolKind::Ordinary,
            workers: 2,
            host: Some(HostIoFailure::from(&host)),
        });

        let source = std::error::Error::source(&error)
            .unwrap_or_else(|| panic!("worker-pool creation must retain its host cause"));

        let source = source
            .downcast_ref::<HostIoFailure>()
            .unwrap_or_else(|| panic!("worker-pool host cause must retain its typed payload"));

        assert_eq!(source.to_string(), expected);
        assert_eq!(source.kind, host.kind());
        assert_eq!(source.raw_os_error, host.raw_os_error());
    }
}
