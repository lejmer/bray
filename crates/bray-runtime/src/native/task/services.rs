use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use bray_runtime_abi::{NativePanicReport, NativeRunOutcome, NativeRunState, NativeRuntimeStatus, NativeTaskReference, NativeWorkerFailure};

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
    worker_failure: OnceLock<NativeWorkerFailure>,
    worker_failure_delivered: AtomicBool,
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
            worker_failure: OnceLock::new(),
            worker_failure_delivered: AtomicBool::new(false),
        })
    }

    pub(crate) fn record_worker_failure(&self, failure: NativeWorkerFailure, scope: Option<&bray_runtime_abi::NativeWorkerScope>) {
        if self.worker_failure.set(failure).is_ok() {
            self.registration.retain_driver(scope, true);
        }
    }

    pub(in crate::native) fn take_worker_failure(&self) -> Option<NativePanicReport> {
        let failure = self.worker_failure.get()?;

        if self.worker_failure_delivered.swap(true, Ordering::Relaxed) {
            return None;
        }

        let mut outcome = NativeRunOutcome::new(NativeRunState::PENDING, 0);

        super::super::workers::failure_reporting(failure, &mut outcome);

        Some(outcome.take_report())
    }

    pub(in crate::native) fn has_worker_failure(&self) -> bool {
        self.pending_worker_failure().is_some()
    }

    pub(in crate::native) fn pending_worker_failure(&self) -> Option<&NativeWorkerFailure> {
        let failure = self.worker_failure.get()?;

        (!self.worker_failure_delivered.load(Ordering::Relaxed)).then_some(failure)
    }

    pub(in crate::native) fn acknowledge_worker_failure(&self) {
        self.worker_failure_delivered.store(true, Ordering::Relaxed);
    }
}
