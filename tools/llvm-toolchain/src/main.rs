#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    bray_llvm_toolchain::run(std::env::args().skip(1))
}
