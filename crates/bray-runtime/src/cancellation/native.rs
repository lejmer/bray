use bray_runtime_abi::NativeTaskReference;

use crate::{CancellationObservation, TaskWakeHandle, report_provider};

/// One native storage reference, shared by dispatch and external requesters.
/// Request state and shield depth belong exclusively to the Bray control cell.
#[derive(Debug)]
pub(super) struct NativeCancellation {
    pub(super) reference: NativeTaskReference,
    pub(super) wake: TaskWakeHandle,
}

impl NativeCancellation {
    pub(super) fn operation(&self, operation: u32) -> u32 {
        report_provider::bray_runtime_task_control_cancellation(&self.reference, operation)
    }

    pub(super) fn request(&self) -> bool {
        let state = self.operation(1);

        if state & 2 != 0 {
            // Scheduler wake transports readiness. Bray owns request and shield policy.
            let _ = self.wake.wake_cancellation();
        }

        state & 1 != 0
    }

    pub(super) fn observation(&self) -> CancellationObservation {
        let state = self.operation(0);

        CancellationObservation::new(state & 1 != 0, state & 2 != 0)
    }
}

impl Drop for NativeCancellation {
    fn drop(&mut self) {
        let status = report_provider::bray_runtime_task_control_reference_release(&self.reference);

        assert!(status.is_success(), "the last cancellation or dispatch user releases its retained Bray cell");
    }
}
