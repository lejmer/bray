use std::convert::Infallible;

use bray_runtime_abi::{NativePanicPrimary, NativeRunOutcome};

use crate::RootCancellationHandle;

pub(in crate::native) const fn active() -> bool {
    false
}

pub(in crate::native) fn with_output<T>(callback: impl FnOnce() -> T) -> T {
    callback()
}

pub(in crate::native) fn register_timeout(_cancellation: RootCancellationHandle) {}

pub(in crate::native) fn record_outcome(_outcome: &NativeRunOutcome) {}

pub(in crate::native) fn record_report(
    _primary: &NativePanicPrimary,
    _identity: Option<bray_runtime_abi::NativeTypeIdentity>,
) {
}

pub(in crate::native) fn record_returned_error() {}

pub(in crate::native) fn record_cleanup_failure(_count: usize) {}

pub(in crate::native) const fn finish() -> Result<(), Infallible> {
    Ok(())
}
