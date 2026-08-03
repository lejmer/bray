use std::path::{Path, PathBuf};

use bray_diagnostics::DiagnosticBag;
use bray_target::TargetIdentity;

use crate::tack::error::operation_diagnostics;

const TOOLCHAIN_ROOT_ENVIRONMENT_VARIABLE: &str = "BRAY_TOOLCHAIN_ROOT";
const STANDARD_LIBRARY_DIRECTORY: &str = "standard-library";
const RUNTIME_DIRECTORY: &str = "runtime";
const RUNTIME_METADATA_FILE_NAME: &str = "bray-runtime.brayrt";

pub(super) struct Toolchain {
    root: PathBuf,
}

impl Toolchain {
    pub(super) fn select(explicit_root: Option<PathBuf>) -> Result<Self, DiagnosticBag> {
        let root = explicit_root
            .or_else(|| std::env::var_os(TOOLCHAIN_ROOT_ENVIRONMENT_VARIABLE).map(PathBuf::from))
            .map(Ok)
            .unwrap_or_else(default_toolchain_root)?;

        let root =
            std::path::absolute(root).map_err(|_| operation_diagnostics("toolchain_root"))?;

        Ok(Self { root })
    }

    pub(super) fn standard_library_root(&self) -> PathBuf {
        self.library_root().join(STANDARD_LIBRARY_DIRECTORY)
    }

    pub(super) fn runtime_metadata(&self, target: &TargetIdentity) -> PathBuf {
        self.library_root()
            .join(RUNTIME_DIRECTORY)
            .join(target.as_str())
            .join(RUNTIME_METADATA_FILE_NAME)
    }

    fn library_root(&self) -> PathBuf {
        self.root.join("lib").join("bray")
    }
}

fn default_toolchain_root() -> Result<PathBuf, DiagnosticBag> {
    let executable =
        std::env::current_exe().map_err(|_| operation_diagnostics("toolchain_executable"))?;

    let directory = executable
        .parent()
        .ok_or_else(|| operation_diagnostics("toolchain_executable"))?;

    if directory.file_name().is_some_and(|name| name == "bin") {
        return directory
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| operation_diagnostics("toolchain_executable"));
    }

    Ok(directory.to_path_buf())
}

#[cfg(test)]
mod tests {
    use bray_target::TargetIdentity;

    use super::Toolchain;

    #[test]
    fn explicit_roots_select_the_stable_installed_layout() {
        let root = std::env::current_dir()
            .unwrap_or_else(|error| panic!("test directory should be available: {error:?}"))
            .join("toolchain");

        let toolchain = Toolchain::select(Some(root.clone()))
            .unwrap_or_else(|diagnostics| panic!("root should resolve: {diagnostics:?}"));

        let target = TargetIdentity::try_new("x86_64-unknown-linux-gnu")
            .unwrap_or_else(|| panic!("test target should be valid"));

        assert_eq!(
            toolchain.standard_library_root(),
            root.join("lib").join("bray").join("standard-library")
        );

        assert_eq!(
            toolchain.runtime_metadata(&target),
            root.join("lib")
                .join("bray")
                .join("runtime")
                .join("x86_64-unknown-linux-gnu")
                .join("bray-runtime.brayrt")
        );
    }
}
