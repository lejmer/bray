use bray_runtime::native::implementation;

use crate::test_host;

native_adapter! {
    pub extern "C" fn bray_runtime_test_entry_selection(entry: u32) -> u8 {
        implementation::register_host_callbacks(test_host::callbacks());

        u8::from(test_host::select_entry(entry))
    }
}
