use bray_runtime::native::implementation;
use bray_runtime_abi::{NativeRuntimeEventCallback, NativeRuntimeStatus};

native_adapter! {
    pub extern "C" fn bray_runtime_event_v1(
        callback: NativeRuntimeEventCallback,
        context: usize,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_event_v1(callback, context)
    }
}
