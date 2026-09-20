use bray_runtime::native::implementation;
use bray_runtime_abi::{NativeRuntimeEventCallback, NativeRuntimeStatus};

native_adapter! {
    pub extern "C" fn bray_runtime_event(
        callback: NativeRuntimeEventCallback,
        context: usize,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_event(callback, context)
    }
}
