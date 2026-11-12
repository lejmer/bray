#[cfg(feature = "test-output")]
mod selected;
#[cfg(not(feature = "test-output"))]
mod unselected;

#[cfg(feature = "test-output")]
pub(crate) use selected::record_cleanup_failure;
#[cfg(feature = "test-output")]
pub use selected::{NativeHostCallbacks, register_host_callbacks};
#[cfg(feature = "test-output")]
pub(super) use selected::{
    active, finish, record_outcome, record_report, record_returned_error, register_timeout,
    with_output,
};
#[cfg(not(feature = "test-output"))]
pub(crate) use unselected::record_cleanup_failure;
#[cfg(not(feature = "test-output"))]
pub(super) use unselected::{
    active, finish, record_outcome, record_report, record_returned_error, register_timeout,
    with_output,
};
