use bray_runtime::native::implementation;
use bray_runtime_abi::{NativeRunOutcome, NativeSynchronousRootCallback};

native_adapter! {
    pub extern "C" fn bray_runtime_foreign_callback_execution(
        callback: NativeSynchronousRootCallback,
        destination: usize,
    ) -> NativeRunOutcome {
        implementation::bray_runtime_foreign_callback_execution(callback, destination)
    }
}
