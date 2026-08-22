#![forbid(unsafe_code)]

mod bundle;
mod command;
mod compiler_known;
mod dependency_audit;
mod input_identity;
mod json;
mod link_map;
mod native_archive;
mod native_product;
mod native_toolchain;
mod package_interface;
mod path;
mod performance;
mod progress;
mod readiness;
mod runtime_artifact;
mod source_format;
mod standard_library;
mod style;
mod workspace;

const USAGE: &str = "usage: cargo xtask <compiler-known | format | package-interface | performance | readiness | runtime-artifact | standard-library | style> ...";

fn main() -> std::process::ExitCode {
    let mut arguments = std::env::args().skip(1);

    match arguments.next().as_deref() {
        Some("compiler-known") => compiler_known::run(arguments),
        Some("format") => source_format::run(arguments),
        Some("package-interface") => package_interface::run(arguments),
        Some("performance") => performance::run(arguments),
        Some("readiness") => readiness::run(arguments),
        Some("runtime-artifact") => runtime_artifact::run(arguments),
        Some("standard-library") => standard_library::run(arguments),
        Some("style") => style::run(arguments),
        _ => {
            eprintln!("{USAGE}");

            std::process::ExitCode::FAILURE
        }
    }
}
