//! Provisioning for the pinned LLVM development toolchain used by Bray.

#![forbid(unsafe_code)]

mod command;
mod instrumentation;
mod process;
#[cfg(test)]
mod test_support;
mod workspace;

pub use command::{run, tool_path};
pub use workspace::root as workspace_root;
