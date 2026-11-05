use bray_runtime::native::implementation;
use bray_runtime_abi::{NativeProductExecution, NativeRuntimeStatus};


native_adapter! {
    pub extern "C" fn bray_runtime_product_execution_provider(
        asynchronous: u32,
        output: &mut std::mem::MaybeUninit<NativeProductExecution>,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_product_execution_provider(asynchronous, output)
    }
}




