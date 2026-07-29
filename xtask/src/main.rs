#![forbid(unsafe_code)]

mod command;
mod compiler_known;
mod digest;
mod llvm;
mod package_interface;
mod readiness;
mod runtime_artifact;
mod style;
mod workspace;

const USAGE: &str =
    "usage: cargo xtask <compiler-known | llvm | package-interface | readiness | runtime-artifact | style> ...";

fn main() -> std::process::ExitCode {
    let mut arguments = std::env::args().skip(1);

    match arguments.next().as_deref() {
        Some("compiler-known") => compiler_known::run(arguments),
        Some("llvm") => llvm::run(arguments),
        Some("package-interface") => package_interface::run(arguments),
        Some("readiness") => readiness::run(arguments),
        Some("runtime-artifact") => runtime_artifact::run(arguments),
        Some("style") => style::run(arguments),
        _ => {
            eprintln!("{USAGE}");

            std::process::ExitCode::FAILURE
        }
    }
}
