use bray_runtime::native::implementation;
use bray_runtime_abi::{
    NativeSynchronousRootCallback, NativeThreadCancellationCallback, NativeThreadOperationCallback,
};

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
    pub extern "C" fn bray_runtime_foreign_callback_execution(
        callback: NativeSynchronousRootCallback,
        context: usize,
    ) -> bray_runtime_abi::NativeRunOutcome {
        implementation::bray_runtime_foreign_callback_execution(callback, context)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_native_thread_execution(
        callback: NativeThreadOperationCallback,
        context: usize,
        cancellation: NativeThreadCancellationCallback,
        cancellation_context: usize,
        panic_report: &mut bray_runtime_abi::NativePanicReport,
    ) -> u32 {
        implementation::bray_runtime_native_thread_execution(
            callback,
            context,
            cancellation,
            cancellation_context,
            panic_report,
        )
    }
}
