use bray_runtime_abi::{
    NativeRunOutcome, NativeRunState, NativeRuntimeStatus, NativeTaskHandle, NativeTaskSnapshot,
};

use crate::{FrameExecutionState, TaskId, report_provider};

use super::services::NativeTaskServices;

impl NativeTaskServices {
    pub(in crate::native) fn handle(&self) -> NativeTaskHandle {
        self.control().task()
    }

    pub(crate) fn id(&self) -> TaskId {
        TaskId::from_bray_handle(self.handle())
    }

    pub(in crate::native) fn snapshot(&self) -> NativeTaskSnapshot {
        let mut snapshot = NativeTaskSnapshot::default();

        let status =
            report_provider::bray_runtime_task_control_snapshot(self.control(), &mut snapshot);

        assert!(status.is_success(), "a retained task cell remains readable");

        snapshot
    }

    pub(in crate::native) fn execution_state(&self) -> FrameExecutionState {
        super::super::activation::execution(&self.snapshot().execution)
    }

    pub(in crate::native) fn observe(&self) -> Result<NativeRunOutcome, NativeRuntimeStatus> {
        let mut outcome = NativeRunOutcome::new(NativeRunState::PENDING, 0);

        let status =
            report_provider::bray_runtime_task_control_observation(self.control(), &mut outcome);

        if status.is_success() {
            Ok(outcome)
        } else {
            Err(status)
        }
    }

    pub(in crate::native) fn resolve_source_owner(&self) -> NativeRuntimeStatus {
        report_provider::bray_runtime_task_control_destruction(self.control())
    }
}
