use bray_runtime::native::implementation;
use bray_runtime_abi::{
    NativeRuntimeStatus, NativeSynchronousRootCallback, NativeThreadCancellationCallback,
    NativeThreadOperationCallback,
};

native_adapter! {
    pub extern "C" fn bray_runtime_substrate_panic_report_initialization(report: &mut bray_runtime_abi::NativePanicReport) -> NativeRuntimeStatus {
        implementation::bray_runtime_substrate_panic_report_initialization(report)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_current_native_thread_identity() -> u64 {
        implementation::bray_runtime_current_native_thread_identity()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_main_native_thread_identity() -> u64 {
        implementation::bray_runtime_main_native_thread_identity()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_substrate_synchronous_root_execution(
        callback: NativeSynchronousRootCallback,
        context: usize,
        cleanup: *const (),
    ) -> bray_runtime_abi::NativeRunOutcome {
        implementation::bray_runtime_substrate_synchronous_root_execution(
            callback,
            context,
            cleanup,
        )
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_substrate_foreign_callback_execution(
        callback: NativeSynchronousRootCallback,
        context: usize,
        cleanup: *const (),
    ) -> bray_runtime_abi::NativeRunOutcome {
        implementation::bray_runtime_substrate_foreign_callback_execution(
            callback,
            context,
            cleanup,
        )
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_substrate_native_thread_execution(
        callback: NativeThreadOperationCallback,
        context: usize,
        cancellation: NativeThreadCancellationCallback,
        cancellation_context: usize,
        panic_report: &mut bray_runtime_abi::NativePanicReport,
        cleanup: *const (),
    ) -> u32 {
        implementation::bray_runtime_substrate_native_thread_execution(
            callback,
            context,
            cancellation,
            cancellation_context,
            panic_report,
            cleanup,
        )
    }
}
