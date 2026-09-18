use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);

/// Process-local identity of one task-control block.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TaskId(NonZeroU64);

impl TaskId {
    pub(crate) const fn from_native(value: NonZeroU64) -> Self {
        Self(value)
    }

    /// Returns the process-local numeric identity.
    pub const fn raw(self) -> u64 {
        self.0.get()
    }
}

/// Failure to create stable task-owned storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskStartError {
    /// Process-local task identities were exhausted.
    IdentityExhausted,
    /// Outgoing incident storage could not be reserved before ownership transfer.
    OutgoingStorageUnavailable,
}

/// Outgoing incident storage acquired before a task accepts its inactive frame.
pub struct TaskAdmission {
    pub(super) id: TaskId,
    pub(crate) outgoing: crate::outgoing::OutgoingRecords,
}

impl TaskAdmission {
    /// Reserves the resume and failure-cleanup bridges and failures from broadcast, lifecycle resolution, result disposal and frame disposal.
    pub fn new() -> Result<Self, TaskStartError> {
        let id = next_task_id()?;

        let outgoing = crate::outgoing::OutgoingRecords::admit(6)
            .map_err(|_| TaskStartError::OutgoingStorageUnavailable)?;

        Ok(Self { id, outgoing })
    }
}

fn next_task_id() -> Result<TaskId, TaskStartError> {
    let id = NEXT_TASK_ID
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map_err(|_| TaskStartError::IdentityExhausted)?;

    NonZeroU64::new(id)
        .map(TaskId)
        .ok_or(TaskStartError::IdentityExhausted)
}
