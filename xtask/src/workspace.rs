use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub(crate) use bray_llvm_toolchain::workspace_root as root;

/// Creates a private directory beneath the repository's `target/test-tmp` for test outputs.
///
/// Test outputs, including built fixture executables, must stay inside the repository.
#[cfg(any(test, feature = "full"))]
pub(crate) fn test_directory(prefix: &str) -> std::io::Result<tempfile::TempDir> {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must live directly beneath the repository root")
        .join("target/test-tmp");

    std::fs::create_dir_all(&parent)?;

    tempfile::Builder::new().prefix(prefix).tempdir_in(parent)
}

#[cfg(test)]
pub(crate) fn temporary_directory() -> tempfile::TempDir {
    test_directory(".tmp").expect("test temporary directory must be created")
}

pub(crate) fn cargo_target(root: &Path) -> PathBuf {
    cargo_target_from(root, std::env::var_os("CARGO_TARGET_DIR"))
}

fn cargo_target_from(root: &Path, configured: Option<OsString>) -> PathBuf {
    let Some(configured) = configured.filter(|value| !value.is_empty()) else {
        return root.join("target");
    };

    let configured = PathBuf::from(configured);

    if configured.is_absolute() {
        return configured;
    }

    root.join(configured)
}

#[cfg(feature = "full")]
pub(crate) fn io_error(action: &str, path: &Path, error: std::io::Error) -> String {
    format!("failed to {action} {}: {error}", path.display())
}

pub(crate) fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?;

    for entry in entries {
        let entry = entry.map_err(|error| format!("could not read directory entry: {error}"))?;
        let path = entry.path();

        if path.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;

    use super::cargo_target_from;

    #[test]
    fn cargo_target_uses_the_workspace_default_without_an_override() {
        assert_eq!(
            cargo_target_from(Path::new("workspace"), None),
            Path::new("workspace").join("target")
        );
    }

    #[test]
    fn cargo_target_resolves_relative_overrides_from_the_workspace() {
        assert_eq!(
            cargo_target_from(
                Path::new("workspace"),
                Some(OsString::from("artifacts/cargo")),
            ),
            Path::new("workspace").join("artifacts/cargo")
        );
    }

    #[test]
    fn cargo_target_preserves_absolute_overrides() {
        let absolute = Path::new(env!("CARGO_MANIFEST_DIR")).join("bray-cargo-target");

        assert_eq!(
            cargo_target_from(
                Path::new("workspace"),
                Some(absolute.clone().into_os_string())
            ),
            absolute
        );
    }
}
