#![forbid(unsafe_code)]

mod command;
mod configuration;
mod diagnostic;

use std::process::ExitCode;

fn main() -> ExitCode {
    command::run(std::env::args_os())
}
