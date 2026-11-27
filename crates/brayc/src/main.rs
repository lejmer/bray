#![forbid(unsafe_code)]

use std::process::ExitCode;

// The checker allocates many short-lived shared values from every worker.
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    bray_driver::run(std::env::args_os())
}
