//! Provisioning for the pinned LLVM development toolchain used by Bray.

#![forbid(unsafe_code)]

mod command;
mod instrumentation;
mod workspace;

pub use command::{run, tool_path};
