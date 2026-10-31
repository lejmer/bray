use std::sync::Mutex;

use bray_runtime_abi::NativeTaskReference;

use crate::context::{TaskOutput, current_task_output};
use crate::{
    CancellationContext, CancellationShield, RuntimeEvent, RuntimeEventGeneration,
    RuntimeEventRegistration, TaskRegistration,
};

/// Native execution resources borrowed by the Source-owned task record.
/// No frame, terminal payload, or source resolution claim is stored here.
pub(crate) struct NativeTaskServices {
    pub(in crate::native) control: NativeTaskReference,
    pub(in crate::native) cancellation: CancellationContext,
    pub(in crate::native) output: TaskOutput,
    pub(in crate::native) registration: TaskRegistration,
    pub(in crate::native) suspended_wait: Mutex<Option<SuspendedWait>>,
    pub(in crate::native) terminal_shield: Mutex<Option<CancellationShield>>,
}

impl std::fmt::Debug for NativeTaskServices {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("NativeTaskServices")
            .field("task", &self.handle())
            .finish_non_exhaustive()
    }
}

impl NativeTaskServices {
    pub(in crate::native) fn new(
        control: NativeTaskReference,
        cancellation: CancellationContext,
        registration: TaskRegistration,
    ) -> Self {
        Self {
            control,
            cancellation,
            output: current_task_output(),
            registration,
            suspended_wait: Mutex::new(None),
            terminal_shield: Mutex::new(None),
        }
    }
}

pub(in crate::native) enum SuspendedWait {
    Event {
        event: RuntimeEvent,
        observed: RuntimeEventGeneration,
        _registration: RuntimeEventRegistration,
    },
}

impl SuspendedWait {
    pub(in crate::native) fn is_ready(&self) -> bool {
        match self {
            Self::Event {
                event, observed, ..
            } => {
                let (generation, closed) = event.observation();

                closed || generation != *observed
            }
        }
    }
}
