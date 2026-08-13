use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use bray_runtime_abi::NativePlatformStatus;

use crate::platform_io_error;

/// Returns the process working directory observed by the native provider at first use.
pub fn startup_working_directory() -> Result<&'static Path, NativePlatformStatus> {
    static DIRECTORY: OnceLock<Result<PathBuf, NativePlatformStatus>> = OnceLock::new();

    match DIRECTORY.get_or_init(|| env::current_dir().map_err(|error| platform_io_error(&error))) {
        Ok(path) => Ok(path),
        Err(status) => Err(*status),
    }
}
