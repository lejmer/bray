use bray_runtime_abi::NativeRuntimeStatus;

use super::binding::current_native_task;
use super::core::NativeRuntime;

impl NativeRuntime {
    pub(in crate::native) fn request_awaited_cancellation(&self) -> NativeRuntimeStatus {
        let Some(parent) = current_native_task() else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        self.with_started(parent, |task| task.run.request_child_cancellation())
            .unwrap_or_else(|status| status)
    }
}
