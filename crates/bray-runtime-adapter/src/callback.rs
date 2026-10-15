use bray_runtime::native::implementation;
use bray_runtime_abi::{NativeThreadCancellationCallback, NativeThreadOperationCallback};

native_adapter! {
    pub extern "C" fn bray_runtime_native_thread_execution(
        callback: NativeThreadOperationCallback,
        context: usize,
        cancellation: NativeThreadCancellationCallback,
        cancellation_context: usize,
        panic_report: &mut std::mem::MaybeUninit<bray_runtime_abi::NativePanicReport>,
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
