//! Native implementation of Bray's platform-service ABI.

#![deny(unsafe_code)]

#[macro_use]
mod boundary;
mod clock;
mod entropy;
mod filesystem;
mod platform;
mod region;

pub use platform::initialize_process_context;
