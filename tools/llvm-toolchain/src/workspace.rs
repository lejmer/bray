use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

/// Locates the Bray Cargo workspace containing the current working directory.
///
/// Returns an error outside a Bray checkout or when Cargo cannot resolve its workspace.
pub fn root() -> Result<PathBuf, String> {
    let directory = std::env::current_dir()
        .map_err(|error| format!("could not read the working directory: {error}"))?;

    root_from(&directory)
}

fn root_from(directory: &Path) -> Result<PathBuf, String> {
    let output = Command::new("cargo")
        .current_dir(directory)
        .args(["locate-project", "--workspace", "--offline"])
        .output()
        .map_err(|error| format!("could not locate the Cargo workspace: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "could not locate the Cargo workspace from {}: {}: {}",
            directory.display(),
            output.status,
            crate::process::output_detail(&output)
        ));
    }

    let location: CargoProject = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("could not decode the Cargo workspace location: {error}"))?;

    let root = location
        .root
        .parent()
        .ok_or("Cargo workspace manifest has no parent directory")?;

    if !root.join("xtask/Cargo.toml").is_file() || !root.join("toolchains/llvm.json").is_file() {
        return Err(format!("{} is not a Bray workspace", root.display()));
    }

    Ok(root.to_path_buf())
}

#[derive(Deserialize)]
struct CargoProject {
    root: PathBuf,
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::root_from;

    fn workspace(parent: &Path, name: &str) -> PathBuf {
        let root = parent.join(name);
        fs::create_dir_all(root.join("xtask/src")).unwrap();
        fs::create_dir_all(root.join("toolchains")).unwrap();
        fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = [\"xtask\"]\n").unwrap();

        fs::write(
            root.join("xtask/Cargo.toml"),
            "[package]\nname = \"xtask\"\nversion = \"0.0.0\"\n",
        )
        .unwrap();

        fs::write(root.join("xtask/src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.join("toolchains/llvm.json"), "{}\n").unwrap();

        root
    }

    #[test]
    fn alternating_workspaces_and_member_directories_select_the_callers_checkout() {
        let directory = tempfile::tempdir().unwrap();
        let first = workspace(directory.path(), "first checkout");
        let second = workspace(directory.path(), "second checkout");

        for root in [&first, &second, &first] {
            for current in [root.clone(), root.join("xtask/src")] {
                assert_eq!(
                    root_from(&current).unwrap().canonicalize().unwrap(),
                    root.canonicalize().unwrap()
                );
            }
        }
    }

    #[test]
    fn cargo_discovery_failures_preserve_the_manifest_error() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("Cargo.toml"), "[workspace").unwrap();

        let error = root_from(directory.path()).unwrap_err();

        assert!(error.contains("Cargo.toml"), "{error}");
        assert!(error.contains("could not locate the Cargo workspace"), "{error}");
        assert!(error.contains("[workspace"), "{error}");
    }

    #[test]
    fn unrelated_cargo_workspaces_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("Cargo.toml"), "[workspace]\n").unwrap();

        let error = root_from(directory.path()).unwrap_err();

        assert!(error.ends_with("is not a Bray workspace"), "{error}");
    }

    #[test]
    fn directories_without_a_cargo_workspace_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let error = root_from(directory.path()).unwrap_err();

        assert!(error.contains("could not locate the Cargo workspace"), "{error}");
        assert!(error.contains("Cargo.toml"), "{error}");
    }
}
