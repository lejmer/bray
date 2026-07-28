#![forbid(unsafe_code)]

mod command;
mod compiler_known;
mod package_interface;
mod runtime_artifact;
mod style;
mod workspace;

const USAGE: &str =
    "usage: cargo xtask <compiler-known | package-interface | runtime-artifact | style> ...";

fn main() -> std::process::ExitCode {
    let mut arguments = std::env::args().skip(1);

    match arguments.next().as_deref() {
        Some("compiler-known") => compiler_known::run(arguments),
        Some("package-interface") => package_interface::run(arguments),
        Some("runtime-artifact") => runtime_artifact::run(arguments),
        Some("style") => style::run(arguments),
        _ => {
            eprintln!("{USAGE}");

            std::process::ExitCode::FAILURE
        }
    }
}
