use std::sync::Mutex;

use bray_runtime_abi::NativeTaskReference;

use crate::context::{TaskOutput, current_task_output};
use crate::{
    CancellationContext, RuntimeEvent, RuntimeEventGeneration, RuntimeEventRegistration,
    TaskRegistration,
};

/// Native execution resources borrowed by the Source-owned task record.
/// No frame, terminal payload, or source resolution claim is stored here.
pub(crate) struct NativeTaskServices {
    pub(crate) cancellation: CancellationContext,
    pub(in crate::native) output: TaskOutput,
    pub(in crate::native) registration: TaskRegistration,
    pub(in crate::native) suspended_wait: Mutex<Option<SuspendedWait>>,
}

impl std::fmt::Debug for NativeTaskServices {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeTaskServices")
            .field("task", &self.handle())
            .finish_non_exhaustive()
    }
}

impl NativeTaskServices {
    pub(in crate::native) fn control(&self) -> &NativeTaskReference {
        self.cancellation.native_reference()
    }

    pub(in crate::native) fn new(
        cancellation: CancellationContext,
        registration: TaskRegistration,
    ) -> Self {
        Self {
            cancellation,
            output: current_task_output(),
            registration,
            suspended_wait: Mutex::new(None),
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
