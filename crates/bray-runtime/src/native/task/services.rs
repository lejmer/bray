use std::sync::{Arc, Mutex};

use bray_runtime_abi::{NativeRuntimeStatus, NativeTaskReference};

use super::super::state::NativeRuntimeCore;
use crate::context::{TaskOutput, current_task_output};
use crate::event::{EventRegistration, NativeCallback};
use crate::{CancellationContext, TaskRegistration};

/// Native execution resources borrowed by the Source-owned task record.
/// No frame, terminal payload, or source resolution claim is stored here.
pub(crate) struct NativeTaskServices {
    pub(crate) callback: NativeCallback,
    pub(crate) cancellation: CancellationContext,
    pub(in crate::native) output: TaskOutput,
    pub(in crate::native) registration: TaskRegistration,
    pub(in crate::native) suspended_wait: Mutex<Option<EventRegistration>>,
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
        owner: &Arc<NativeRuntimeCore>,
    ) -> Result<Self, NativeRuntimeStatus> {
        let callback = NativeCallback::admit(cancellation.clone(), registration.wake_handle(), owner)?;

        Ok(Self {
            callback,
            cancellation,
            output: current_task_output(),
            registration,
            suspended_wait: Mutex::new(None),
        })
    }
}
