#![forbid(unsafe_code)]

mod compiler_known;
mod package_interface;

const USAGE: &str = "usage: cargo xtask <compiler-known | package-interface> ...";

fn main() -> std::process::ExitCode {
    let mut arguments = std::env::args().skip(1);

    match arguments.next().as_deref() {
        Some("compiler-known") => compiler_known::run(arguments),
        Some("package-interface") => package_interface::run(arguments),
        _ => {
            eprintln!("{USAGE}");
            std::process::ExitCode::FAILURE
        }
    }
}
