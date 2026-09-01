use std::io;

use bray_bound_tree::{BoundUnitId, BoundUnitKey};

use super::{
    CompilationFactKey, CompilationInputKey, FactFingerprint, FactTaskIdentity, RuntimeIdentity,
};

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
    /// A compilation input fingerprint was unavailable or changed during one task.
    FingerprintFailure,
    /// Cache or dependency publication state did not match the requested operation.
    PublicationMismatch,
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
        write!(formatter, "compiler query runtime failure: {:?}", self.cause())
    }
}

impl std::error::Error for FactRuntimeError {}

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
        host: Option<io::ErrorKind>,
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
    MissingInputFingerprint {
        input: CompilationInputKey,
        task: FactTaskIdentity,
        fact: CompilationFactKey,
    },
    InputFingerprintMismatch {
        input: CompilationInputKey,
        expected: FactFingerprint,
        actual: FactFingerprint,
        task: FactTaskIdentity,
        fact: CompilationFactKey,
    },
    PublicationMismatch {
        requested: PublicationIdentity,
        actual: PublicationState,
    },
    AbandonedComputation {
        task: FactTaskIdentity,
        fact: CompilationFactKey,
    },
    MissingDependencyRecord {
        task: FactTaskIdentity,
        fact: CompilationFactKey,
        dependency: CompilationFactKey,
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
            | Self::UnitIdentityCapacityExhausted { .. } => {
                FactRuntimeErrorKind::CapacityExhausted
            }
            Self::WorkerPoolCreation { .. } => FactRuntimeErrorKind::WorkerPoolCreation,
            Self::WorkerTerminated { .. } => FactRuntimeErrorKind::WorkerTerminated,
            Self::InvalidTaskState { .. } => FactRuntimeErrorKind::InvalidTaskState,
            Self::InvalidSchedulerState { .. } | Self::SchedulerLocalStateUnavailable { .. } => {
                FactRuntimeErrorKind::InvalidSchedulerState
            }
            Self::InvalidTaskContext { .. } | Self::TaskLocalStateUnavailable { .. } => {
                FactRuntimeErrorKind::InvalidTaskContext
            }
            Self::MissingInputFingerprint { .. } | Self::InputFingerprintMismatch { .. } => {
                FactRuntimeErrorKind::FingerprintFailure
            }
            Self::PublicationMismatch { .. } => FactRuntimeErrorKind::PublicationMismatch,
            Self::AbandonedComputation { .. } => FactRuntimeErrorKind::AbandonedComputation,
            Self::MissingDependencyRecord { .. }
            | Self::InvalidWaitGraph { .. }
            | Self::MissingCycle { .. } => {
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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PublicationIdentity {
    pub(crate) task: Option<FactTaskIdentity>,
    pub(crate) fact: CompilationFactKey,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PublicationState {
    Vacant,
    Computing {
        task: FactTaskIdentity,
        fact: CompilationFactKey,
    },
    Ready {
        fact: Option<CompilationFactKey>,
    },
    PublishedFlagWithoutValue,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum CancellationStateKind {
    Request,
    Shared,
}

#[cfg(test)]
mod tests {
    use super::{FactRuntimeError, FactRuntimeErrorKind, FactRuntimeFailure};

    #[test]
    fn opaque_runtime_errors_keep_query_stack_frames_pointer_sized() {
        assert_eq!(
            std::mem::size_of::<FactRuntimeError>(),
            std::mem::size_of::<usize>()
        );
    }

    #[test]
    fn public_kind_projects_the_private_exact_cause() {
        let error = FactRuntimeError::from(FactRuntimeFailure::WorkerTerminated {
            worker: Some(2),
            item: Some(9),
        });

        assert_eq!(error.kind(), FactRuntimeErrorKind::WorkerTerminated);
    }
}
