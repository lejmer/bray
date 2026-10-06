use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct CargoMetadata {
    packages: Vec<PackageDependencies>,
}

impl CargoMetadata {
    pub(super) fn load(root: &Path) -> Result<Self, String> {
        let mut command = Command::new("cargo");

        // Inspect declarations on every target without resolving or downloading dependencies.
        command.current_dir(root).args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ]);

        let output = crate::command::require_success(command, "inspect readiness dependencies")?;

        let mut metadata: Self = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("could not decode readiness dependencies: {error}"))?;

        for package in &mut metadata.packages {
            package.manifest_path = package.manifest_path.canonicalize().map_err(|error| {
                format!(
                    "could not resolve readiness manifest {}: {error}",
                    package.manifest_path.display()
                )
            })?;
        }

        Ok(metadata)
    }

    pub(super) fn package(&self, manifest: &Path) -> Result<&PackageDependencies, String> {
        self.packages
            .iter()
            .find(|package| package.manifest_path == manifest)
            .ok_or_else(|| {
                format!(
                    "could not find readiness dependencies for {}",
                    manifest.display()
                )
            })
    }
}

#[derive(Deserialize)]
pub(super) struct PackageDependencies {
    manifest_path: PathBuf,
    dependencies: Vec<Dependency>,
}

impl PackageDependencies {
    // Build dependencies also belong to the production boundary, even on another target.
    pub(super) fn contains_production(&self, name: &str) -> bool {
        self.dependencies.iter().any(|dependency| {
            dependency.name == name && dependency.kind != Some(DependencyKind::Dev)
        })
    }

    pub(super) fn contains_normal(&self, name: &str) -> bool {
        self.dependencies
            .iter()
            .any(|dependency| dependency.name == name && dependency.kind.is_none())
    }
}

#[derive(Deserialize)]
struct Dependency {
    name: String,
    kind: Option<DependencyKind>,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum DependencyKind {
    Dev,
    Build,
}

#[cfg(test)]
mod tests {
    use super::super::workspace::tests::dependency_workspace;

    #[test]
    fn cargo_dependency_identity_ignores_text_and_preserves_declaration_kinds() {
        let (_directory, workspace) = dependency_workspace(&[(
            "boundary",
            r#"
            [package.metadata.readiness]
            note = "bray-checker"

            [dev-dependencies]
            bray-checker = "1"

            [target.'cfg(windows)'.dev-dependencies]
            bray-bound-tree = "1"

            [dependencies]
            mir.workspace = true
            bray-checker-support = "1"
            checker = { package = "bray-binder", version = "1", optional = true }

            [build-dependencies]
            bray-parser = "1"

            [target.'cfg(windows)'.dependencies]
            syntax = { package = "bray-syntax", version = "1" }

            [target.'cfg(windows)'.build-dependencies]
            bray-source = "1"
        "#,
        )]);

        let dependencies = workspace
            .package_dependencies("crates/boundary/Cargo.toml")
            .expect("package dependencies");

        assert!(!dependencies.contains_production("bray-checker"));
        assert!(!dependencies.contains_production("bray-bound-tree"));
        assert!(!dependencies.contains_production("mir"));
        assert!(!dependencies.contains_production("checker"));
        assert!(dependencies.contains_normal("bray-ir"));
        assert!(dependencies.contains_normal("bray-checker-support"));
        assert!(dependencies.contains_normal("bray-binder"));
        assert!(dependencies.contains_normal("bray-syntax"));
        assert!(dependencies.contains_production("bray-parser"));
        assert!(dependencies.contains_production("bray-source"));
        assert!(!dependencies.contains_normal("bray-parser"));
        assert!(!dependencies.contains_normal("bray-source"));
    }

    #[test]
    fn missing_package_dependencies_fail_instead_of_accepting_an_empty_boundary() {
        let (_directory, workspace) = dependency_workspace(&[("boundary", "")]);

        assert!(
            workspace
                .package_dependencies("crates/missing/Cargo.toml")
                .is_err()
        );
    }
}
