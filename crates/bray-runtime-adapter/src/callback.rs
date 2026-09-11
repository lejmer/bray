use bray_runtime::native::implementation;
use bray_runtime_abi::{
    NativePanicMessageCopyCallback, NativeRuntimeStatus, NativeSynchronousRootCallback,
    NativeThreadCancellationCallback, NativeThreadOperationCallback,
};

native_adapter! {
    pub extern "C" fn bray_runtime_substrate_panic_reporting(
        cause: u32,
        source_present: u32,
        source_identity: u32,
        source_start: u32,
        source_end: u32,
        source_version: u64,
        message: *const u8,
        message_length: usize,
        copy_message: Option<NativePanicMessageCopyCallback>,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_substrate_panic_reporting(
            cause,
            source_present,
            source_identity,
            source_start,
            source_end,
            source_version,
            message,
            message_length,
            copy_message,
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
        context: usize,
    ) -> bray_runtime_abi::NativeRunOutcome {
        implementation::bray_runtime_foreign_callback_execution(
            callback,
            context,
        )
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_native_thread_execution(
        callback: NativeThreadOperationCallback,
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
