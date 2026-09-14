//! Provisioning for the pinned LLVM development toolchain used by Bray.

#![forbid(unsafe_code)]

mod command;
mod instrumentation;
mod process;
mod workspace;

pub use command::{run, tool_path};
pub use workspace::root as workspace_root;
