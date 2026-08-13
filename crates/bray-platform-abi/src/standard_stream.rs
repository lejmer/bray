//! Link anchors for independently compiled target-native standard-stream leaves.

#[expect(
    unsafe_code,
    reason = "the platform ABI links target-native standard-stream leaves"
)]
#[cfg(any(windows, unix))]
#[link(name = "bray_platform_standard_streams")]
unsafe extern "C" {}
