#![forbid(unsafe_code)]

mod compiler_known;

fn main() -> std::process::ExitCode {
    compiler_known::run(std::env::args().skip(1))
}
