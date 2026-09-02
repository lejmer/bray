//! Reusable structural and whitespace conventions for Rust source trees.

#![forbid(unsafe_code)]

mod blank_line;
mod diagnostic;
mod exemption;
mod failure;
mod source;
mod structure;
mod workspace;

pub use workspace::{check_workspace, fix_workspace};
