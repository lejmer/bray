//! Bray Tack project and workspace command driver.

#![forbid(unsafe_code)]

mod driver;
mod formatter;
mod language_server;
mod tack;
#[cfg(test)]
mod test_support;

pub use driver::run;
pub use tack::{
    TackCliError, TackCommandKind, TackDiagnostics, TackFormatInput,
    TackFormatMode, TackFormatRequest, TackFormatService, TackInvocation,
    TackLanguageServerRequest, TackLanguageServerService, TackRunResult,
    TackServiceResult, TackServices, run_tack, run_tack_result,
    run_tack_with_services,
};
