#![forbid(unsafe_code)]

use std::process::ExitCode;

mod formatter;

fn main() -> ExitCode {
    let formatter = formatter::BrayFormatService;
    let services = bray_driver::TackServices::new().with_formatter(&formatter);

    bray_driver::run_tack_with_services(std::env::args_os(), services)
}
