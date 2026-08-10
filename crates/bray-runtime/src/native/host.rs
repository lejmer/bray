#[cfg(not(feature = "test-host"))]
use bray_runtime_abi::{NativePanicCause, NativeRunOutcome, NativeSourceAnchor};

#[cfg(not(feature = "test-host"))]
use crate::RootCancellationHandle;

#[cfg(feature = "test-host")]
pub(super) use super::test::{
    active, finish, record_cleanup_failure, record_outcome, record_panic, record_returned_error,
    register_timeout, with_output,
};

#[cfg(not(feature = "test-host"))]
pub(super) const fn active() -> bool {
    false
}

#[cfg(not(feature = "test-host"))]
pub(super) fn with_output<T>(callback: impl FnOnce() -> T) -> T {
    callback()
}

#[cfg(not(feature = "test-host"))]
pub(super) fn register_timeout(_: RootCancellationHandle) {}

#[cfg(not(feature = "test-host"))]
pub(super) fn record_outcome(_: NativeRunOutcome) {}

#[cfg(not(feature = "test-host"))]
pub(super) fn record_panic(_: NativePanicCause, _: NativeSourceAnchor, _: String) {}

#[cfg(not(feature = "test-host"))]
pub(super) fn record_returned_error() {}

#[cfg(not(feature = "test-host"))]
pub(super) fn record_cleanup_failure(_: usize) {}

#[cfg(not(feature = "test-host"))]
pub(super) const fn finish() -> std::io::Result<()> {
    Ok(())
}
