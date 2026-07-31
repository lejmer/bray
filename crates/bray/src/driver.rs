use std::ffi::OsString;
use std::process::ExitCode;

use crate::formatter::BrayFormatService;
use crate::{TackServices, run_tack_with_services};

/// Runs the Bray Tack command with package-owned tool integrations.
pub fn run(
    arguments: impl IntoIterator<Item = OsString>,
) -> ExitCode {
    let formatter = BrayFormatService;
    let services = TackServices::new().with_formatter(&formatter);

    run_tack_with_services(arguments, services)
}
