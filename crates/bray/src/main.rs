#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    bray::run(std::env::args_os())
}
