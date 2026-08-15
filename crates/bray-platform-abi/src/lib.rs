//! Native implementation of Bray's platform-service ABI.

#![cfg_attr(
    not(any(feature = "core", feature = "filesystem", feature = "process")),
    no_std
)]
#![deny(unsafe_code)]

#[cfg(feature = "core")]
mod clock;
#[cfg(feature = "core")]
mod entropy;
#[cfg(feature = "filesystem")]
mod filesystem;
#[cfg(all(
    not(test),
    feature = "standard-streams",
    not(any(feature = "core", feature = "filesystem", feature = "process"))
))]
mod panic;
#[cfg(feature = "core")]
mod platform;
#[cfg(feature = "process")]
mod process;
#[cfg(feature = "standard-streams")]
mod standard_stream;
#[cfg(feature = "core")]
mod temporal_link;

#[cfg(all(test, feature = "core"))]
mod temporal_tests;

#[cfg(feature = "core")]
pub use platform::initialize_process_context;
