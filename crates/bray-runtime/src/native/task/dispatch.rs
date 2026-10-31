use bray_runtime_abi::{
    NativeActivationExecution, NativeFrameProgress, NativeFrameProgressKind, NativeReportRecords,
    NativeTaskDispatchServices, NativeTaskHandle,
};

use crate::{outgoing::OutgoingRecords, report_provider};

use super::super::{activation, state};
use super::services::NativeTaskServices;

impl NativeTaskServices {
    pub(in crate::native) fn dispatch(&self) -> (u32, NativeFrameProgress, NativeActivationExecution) {
        let services = NativeTaskDispatchServices {
            invoke: activation::bray_runtime_activation_callback,
            ready: activation::readiness,
            transfer,
            wake,
        };

        let mut progress = NativeFrameProgress::new(NativeFrameProgressKind::RUNTIME_FAILURE, 0, 0);
        let mut execution = NativeActivationExecution::default();

        let status = report_provider::bray_runtime_task_control_dispatch(
            self.control(), &services, &mut progress, &mut execution,
        );

        (status, progress, execution)
    }

    pub(in crate::native) fn enter_callback(
        &self,
        execution: &NativeActivationExecution,
        resumable: bool,
    ) -> CallbackScope<'_> {
        let mut previous_resumable = 0;

        let previous = report_provider::bray_runtime_task_control_callback_entry(
            self.control(), execution, u8::from(resumable), &mut previous_resumable,
        );

        CallbackScope { services: self, previous, previous_resumable }
    }
}

pub(in crate::native) struct CallbackScope<'a> {
    services: &'a NativeTaskServices,
    previous: usize,
    previous_resumable: u8,
}

impl Drop for CallbackScope<'_> {
    fn drop(&mut self) {
        report_provider::bray_runtime_task_control_callback_exit(
            self.services.control(), self.previous, self.previous_resumable,
        );
    }
}

pub(super) extern "C" fn transfer(
    execution: &NativeActivationExecution,
    unused: &mut NativeReportRecords,
    incidents: &mut NativeReportRecords,
) {
    let NativeReportRecords { head, tail, count } = std::mem::take(unused);
    let _unused = OutgoingRecords::from_parts(head, tail, count);
    let NativeReportRecords { head, tail, count } = std::mem::take(incidents);

    if count == 0 {
        return;
    }

    let records = OutgoingRecords::from_parts(head, tail, count);
    let execution = activation::execution(execution);
    let origin = crate::CleanupIncidentOrigin::new(execution.frame(), execution.state());
    let task = state::current_services().expect("incident delivery retains its native task");

    state::with_runtime(|runtime| {
        runtime.cleanup_reports.transfer_records(
            crate::CleanupIncidentProducer::Task(task.id()), origin, records,
        );
    }).expect("incident delivery retains its native runtime");
}

extern "C" fn wake(identity: u64) {
    let task = NativeTaskHandle::new(identity).expect("an admitted waiter has a nonzero task identity");

    // A resolved parent can unregister before the last notification returns.
    // Its Bray link still owns storage. A stale native wake has no frame claim.
    let _ = state::with_runtime(|runtime| runtime.wake(task));
}
