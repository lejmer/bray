use bray_runtime::native::implementation;

native_adapter! {
    pub extern "C" fn bray_runtime_activation_callback_admission(
        count: usize,
        destination: &mut bray_runtime_abi::NativeReportRecords,
    ) -> u32 {
        implementation::bray_runtime_activation_callback_admission(count, destination)
    }
}
