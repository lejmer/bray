use std::io;

use bray_bound_tree::BoundUnitKey;

use super::{CompilationFactKey, FactTaskIdentity};

/// Stable category for a compiler query runtime failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FactRuntimeErrorKind {
    /// A bounded runtime identity or counter could not represent another value.
    CapacityExhausted,
    /// A compiler query worker pool could not be created.
    WorkerPoolCreation,
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
    UnitSourceCapacityExhausted {
        source_count: usize,
    },
    UnitIdentityCapacityExhausted {
        unit: BoundUnitKey,
        source_ordinal: u32,
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
            Self::WorkerPoolCreation { .. } => FactRuntimeErrorKind::WorkerPoolCreation,
            Self::CapacityExhausted { .. }
            | Self::UnitSourceCapacityExhausted { .. }
            | Self::UnitIdentityCapacityExhausted { .. } => FactRuntimeErrorKind::CapacityExhausted,
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
    ActiveSlots,
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
        CapacityResource, FactRuntimeError, FactRuntimeErrorKind, FactRuntimeFailure,
        HostIoFailure, WorkerPoolKind,
    };
    use crate::fact::{CompilationFactKey, FactTaskIdentity};
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
