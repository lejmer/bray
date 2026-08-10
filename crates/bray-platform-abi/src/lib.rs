//! Native implementation of Bray's platform-service ABI.

#![deny(unsafe_code)]

mod clock;
mod entropy;
mod filesystem;
mod platform;
mod process;
mod region;
mod temporal_link;

#[cfg(test)]
mod temporal_tests;

pub use platform::initialize_process_context;
