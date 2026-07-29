#[cfg(windows)]
use std::env;
#[cfg(windows)]
use std::error::Error;
#[cfg(windows)]
use std::fs;
#[cfg(windows)]
use std::io;
#[cfg(windows)]
use std::path::{Path, PathBuf};

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn Error>> {
    let prefix = PathBuf::from(env::var("LLVM_SYS_221_PREFIX")?);
    let library_directory = prefix.join("lib");
    let dynamic_library = prefix.join("bin").join("LLVM-C.dll");

    println!("cargo:rustc-link-search=native={}", library_directory.display());
    println!("cargo:rustc-link-lib=dylib=LLVM-C");
    println!("cargo:rerun-if-changed={}", dynamic_library.display());

    let output_directory = PathBuf::from(env::var("OUT_DIR")?);

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
