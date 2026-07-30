//! Bray Tack project and workspace command orchestration.

mod cli;
mod compiler;
mod error;
mod execute;
mod install;
mod inspection;
mod model;
mod project;
mod result;
mod service;

pub use cli::TackCliError;
pub use execute::{run_tack, run_tack_result, run_tack_with_services};
pub use model::{TackCommandKind, TackInvocation};
pub use result::{TackDiagnostics, TackRunResult};
pub use service::{
    TackFormatInput, TackFormatMode, TackFormatRequest, TackFormatService,
    TackLanguageServerRequest, TackLanguageServerService, TackServiceResult,
    TackServices,
};
