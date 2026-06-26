#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    bray_driver::run(std::env::args_os())
}
