use std::ffi::OsString;
use std::process::ExitCode;

use crate::formatter::BrayFormatService;
use crate::language_server::BrayLanguageServerService;
use crate::{TackServices, run_tack_with_services};

/// Runs the Bray Tack command with package-owned tool integrations.
pub fn run(
    arguments: impl IntoIterator<Item = OsString>,
) -> ExitCode {
    let formatter = BrayFormatService;
    let language_server = BrayLanguageServerService;

    let services = TackServices::new()
        .with_formatter(&formatter)
        .with_language_server(&language_server);

    run_tack_with_services(arguments, services)
}
