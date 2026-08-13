#[cfg(feature = "test-output")]
mod selected;
#[cfg(not(feature = "test-output"))]
mod unselected;

#[cfg(feature = "test-output")]
pub use selected::{NativeHostCallbacks, register_host_callbacks};
#[cfg(feature = "test-output")]
pub(super) use selected::{
    active, finish, record_cleanup_failure, record_outcome, record_panic, record_returned_error,
    register_timeout, with_output,
};
#[cfg(not(feature = "test-output"))]
pub(super) use unselected::{
    active, finish, record_cleanup_failure, record_outcome, record_panic, record_returned_error,
    register_timeout, with_output,
};
