use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub(crate) fn root() -> Result<PathBuf, String> {
    let xtask = Path::new(env!("CARGO_MANIFEST_DIR"));

    xtask
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask manifest directory has no workspace parent".to_owned())
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
        let absolute = std::env::temp_dir().join("bray-cargo-target");

        assert_eq!(
            cargo_target_from(
                Path::new("workspace"),
                Some(absolute.clone().into_os_string())
            ),
            absolute
        );
    }
}
