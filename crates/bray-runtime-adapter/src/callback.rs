use bray_runtime::native::implementation;
use bray_runtime_abi::{
    NativeRunOutcome, NativeSynchronousRootCallback, NativeThreadCancellationCallback,
};

native_adapter! {
    pub extern "C" fn bray_runtime_native_thread_execution(
        callback: NativeSynchronousRootCallback,
        context: usize,
        cancellation: NativeThreadCancellationCallback,
        cancellation_context: usize,
        panic_payload: &mut usize,
    ) -> u32 {
        implementation::bray_runtime_native_thread_execution(
            callback,
            context,
            cancellation,
            cancellation_context,
            panic_payload,
        )
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
    pub extern "C" fn bray_runtime_native_thread_panic_report_recovery(payload: usize) -> usize {
        implementation::bray_runtime_native_thread_panic_report_recovery(payload)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_foreign_callback_execution(
        callback: NativeSynchronousRootCallback,
        destination: usize,
    ) -> NativeRunOutcome {
        implementation::bray_runtime_foreign_callback_execution(callback, destination)
    }
}
