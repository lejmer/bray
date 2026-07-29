use std::env;
use std::error::Error;
#[cfg(windows)]
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const LLVM_REVISION: &str = "22.1.8";

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-env-changed=LLVM_SYS_221_PREFIX");

    let prefix = PathBuf::from(env::var("LLVM_SYS_221_PREFIX")?);
    let config = llvm_config(&prefix);
    let version = command_output(&config, "--version")?;

    if version != LLVM_REVISION {
        return Err(io::Error::other(format!(
            "LLVM {LLVM_REVISION} is required, but {version} was selected"
        ))
        .into());
    }

    let targets = command_output(&config, "--targets-built")?;

    println!("cargo:rustc-env=BRAY_LLVM_REVISION={version}");

    println!(
        "cargo:rustc-env=BRAY_LLVM_TARGETS={}",
        targets.split_whitespace().collect::<Vec<_>>().join(",")
    );

    configure_linkage(&prefix)?;

    Ok(())
}

fn llvm_config(prefix: &Path) -> PathBuf {
    let executable = if cfg!(windows) {
        "llvm-config.exe"
    } else {
        "llvm-config"
    };

    prefix.join("bin").join(executable)
}

fn command_output(config: &Path, argument: &str) -> io::Result<String> {
    let output = Command::new(config).arg(argument).output()?;

    if !output.status.success() {
        return Err(io::Error::other(format!(
            "{} {argument} failed",
            config.display()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(not(windows))]
fn configure_linkage(_prefix: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(windows)]
fn configure_linkage(prefix: &Path) -> io::Result<()> {
    let library_directory = prefix.join("lib");
    let dynamic_library = prefix.join("bin").join("LLVM-C.dll");

    println!("cargo:rustc-link-search=native={}", library_directory.display());
    println!("cargo:rustc-link-lib=dylib=LLVM-C");
    println!("cargo:rerun-if-changed={}", dynamic_library.display());

    let output_directory = PathBuf::from(env::var("OUT_DIR").map_err(io::Error::other)?);

    let profile_directory = output_directory
        .ancestors()
        .nth(3)
        .ok_or_else(|| io::Error::other("Cargo OUT_DIR has no profile directory"))?;

    copy_dynamic_library(&dynamic_library, profile_directory)?;
    copy_dynamic_library(&dynamic_library, &profile_directory.join("deps"))?;

    Ok(())
}

#[cfg(windows)]
fn copy_dynamic_library(source: &Path, destination_directory: &Path) -> io::Result<()> {
    fs::create_dir_all(destination_directory)?;

    let destination = destination_directory.join("LLVM-C.dll");

    fs::copy(source, destination)?;

    Ok(())
}
