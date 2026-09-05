//! Bray Tack project and workspace command orchestration.

mod cli;
mod compiler;
mod error;
mod execute;
mod identity;
mod init;
mod inspection;
mod install;
mod model;
mod output;
mod profile;
mod progress;
mod project;
mod result;
mod storage;
mod testing;
mod tool;
mod toolchain;

pub use cli::TackCliError;
pub use execute::{run_tack, run_tack_result};
pub use model::{TackCommandKind, TackInvocation};
pub use result::TackRunResult;
