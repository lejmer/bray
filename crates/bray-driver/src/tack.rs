//! Bray Tack project and workspace command orchestration.

mod cli;
mod compiler;
mod error;
mod execute;
mod install;
mod inspection;
mod model;
mod project;
mod service;

pub use cli::TackCliError;
pub use execute::{
    TackRunResult, run_tack, run_tack_result, run_tack_with_services,
};
pub use model::{TackCommandKind, TackInvocation};
pub use service::{
    TackFormatInput, TackFormatMode, TackFormatRequest, TackFormatService,
    TackLanguageServerRequest, TackLanguageServerService, TackServiceResult,
    TackServices,
};
