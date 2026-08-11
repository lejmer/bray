use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{ProjectLoadError, ProjectPath};
use bray_diagnostics::DiagnosticProjectManifestField;
use bray_standard_library::PackageSourceAuthority;
use bray_symbols::PackageIdentity;

pub(super) fn read_manifest_source(path: &Path) -> Result<String, ProjectLoadError> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| ProjectLoadError::ReadManifest {
            path: path.to_path_buf(),
            kind: error.kind(),
        })?;

    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ProjectLoadError::invalid_path(
            path.to_path_buf(),
            DiagnosticProjectManifestField::ManifestPath,
            path.to_path_buf(),
        ));
    }

    std::fs::read_to_string(path).map_err(|error| ProjectLoadError::ReadManifest {
        path: path.to_path_buf(),
        kind: error.kind(),
    })
}

pub(super) fn require_owned_package_path(
    workspace_root: &Path,
    package_path: &ProjectPath,
    workspace_manifest_path: &Path,
) -> Result<(), ProjectLoadError> {
    if package_path.as_str() == "." {
        return Ok(());
    }

    let mut current = workspace_root.to_path_buf();

    for component in package_path.as_str().split('/') {
        current.push(component);

        let metadata = std::fs::symlink_metadata(&current).map_err(|_| {
            ProjectLoadError::invalid_path(
                workspace_manifest_path.to_path_buf(),
                DiagnosticProjectManifestField::WorkspacePackagePath,
                package_path.as_str().into(),
            )
        })?;

        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(ProjectLoadError::invalid_path(
                workspace_manifest_path.to_path_buf(),
                DiagnosticProjectManifestField::WorkspacePackagePath,
                package_path.as_str().into(),
            ));
        }
    }

    Ok(())
}

pub(super) fn project_path(
    value: String,
    allow_workspace_root: bool,
    manifest_path: &Path,
    field: DiagnosticProjectManifestField,
) -> Result<ProjectPath, ProjectLoadError> {
    ProjectPath::try_new(Arc::<str>::from(value.as_str()), allow_workspace_root).ok_or_else(|| {
        ProjectLoadError::invalid_path(manifest_path.to_path_buf(), field, value.into())
    })
}

pub(super) fn local_name(
    value: String,
    manifest_path: &Path,
    field: DiagnosticProjectManifestField,
) -> Result<Arc<str>, ProjectLoadError> {
    if is_local_name(&value) {
        return Ok(value.into());
    }

    Err(ProjectLoadError::invalid_name(
        manifest_path.to_path_buf(),
        field,
        value,
    ))
}

pub(super) fn package_identity(
    value: String,
    manifest_path: &Path,
    source_authority: PackageSourceAuthority,
    field: DiagnosticProjectManifestField,
) -> Result<PackageIdentity, ProjectLoadError> {
    if !has_valid_package_identity_syntax(&value) {
        return Err(ProjectLoadError::invalid_name(
            manifest_path.to_path_buf(),
            field,
            value,
        ));
    }

    let Some(identity) = PackageIdentity::try_new(Arc::<str>::from(value.as_str())) else {
        return Err(ProjectLoadError::invalid_name(
            manifest_path.to_path_buf(),
            field,
            value,
        ));
    };

    if !source_authority.accepts(&identity) {
        return Err(match source_authority {
            PackageSourceAuthority::Ordinary => ProjectLoadError::reserved_package_identity(
                manifest_path.to_path_buf(),
                field,
                value,
            ),
            PackageSourceAuthority::StandardLibrary => {
                ProjectLoadError::standard_library_package_identity_required(
                    manifest_path.to_path_buf(),
                    field,
                    value,
                )
            }
        });
    }

    Ok(identity)
}

/// Returns whether a package identity is valid for ordinary project source.
pub fn is_valid_ordinary_package_identity(value: &str) -> bool {
    if !has_valid_package_identity_syntax(value) {
        return false;
    }

    PackageIdentity::try_new(Arc::<str>::from(value))
        .is_some_and(|identity| PackageSourceAuthority::Ordinary.accepts(&identity))
}

pub(super) fn sorted_unique_names(
    values: Vec<String>,
    manifest_path: &Path,
    field: DiagnosticProjectManifestField,
) -> Result<Arc<[Arc<str>]>, ProjectLoadError> {
    let mut names = values
        .into_iter()
        .map(|value| local_name(value, manifest_path, field))
        .collect::<Result<Vec<_>, _>>()?;

    names.sort_unstable();

    if let Some(pair) = names.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(ProjectLoadError::duplicate_selection(
            manifest_path.to_path_buf(),
            field,
            pair[0].to_string(),
        ));
    }

    Ok(names.into())
}

pub(super) fn paths_overlap(first: &ProjectPath, second: &ProjectPath) -> bool {
    is_path_prefix(first.as_str(), second.as_str())
        || is_path_prefix(second.as_str(), first.as_str())
}

fn is_local_name(value: &str) -> bool {
    let mut characters = value.chars();

    matches!(characters.next(), Some('a'..='z'))
        && characters.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '_' | '-')
        })
}

fn is_package_segment(value: &str) -> bool {
    is_local_name(value)
}

fn has_valid_package_identity_syntax(value: &str) -> bool {
    value.split('.').all(is_package_segment)
}

fn is_path_prefix(prefix: &str, path: &str) -> bool {
    prefix == path
        || prefix == "."
        || path
            .strip_prefix(prefix)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

pub(super) fn manifest_path(package_root: &Path) -> PathBuf {
    package_root.join(crate::PACKAGE_MANIFEST_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bray_diagnostics::DiagnosticProjectManifestField;
    use bray_standard_library::PackageSourceAuthority;

    use super::{
        is_valid_ordinary_package_identity, local_name, package_identity, paths_overlap,
        project_path,
    };

    #[test]
    fn names_have_one_canonical_ascii_spelling() {
        let manifest = Path::new("bray-package.json");

        let field = DiagnosticProjectManifestField::ProductIdentity;

        assert!(local_name(String::from("native-test"), manifest, field).is_ok());
        assert!(local_name(String::from("Native"), manifest, field).is_err());
        assert!(local_name(String::from("9native"), manifest, field).is_err());

        assert!(
            package_identity(
                String::from("example.math"),
                manifest,
                PackageSourceAuthority::Ordinary,
                DiagnosticProjectManifestField::PackageIdentity,
            )
            .is_ok()
        );

        assert!(
            package_identity(
                String::from("example"),
                manifest,
                PackageSourceAuthority::Ordinary,
                DiagnosticProjectManifestField::PackageIdentity,
            )
            .is_ok()
        );

        assert!(
            package_identity(
                String::from("example.math_core"),
                manifest,
                PackageSourceAuthority::Ordinary,
                DiagnosticProjectManifestField::PackageIdentity,
            )
            .is_ok()
        );

        assert!(is_valid_ordinary_package_identity("example.math"));
        assert!(is_valid_ordinary_package_identity("example.math_core"));
        assert!(is_valid_ordinary_package_identity("example"));
        assert!(!is_valid_ordinary_package_identity("std.io"));
    }

    #[test]
    fn overlap_uses_portable_component_boundaries() {
        let manifest = Path::new("bray-workspace.json");

        let field = DiagnosticProjectManifestField::SourceRootPath;

        let Ok(source) = project_path(String::from("app/src"), false, manifest, field) else {
            panic!("test source path must be valid");
        };

        let Ok(nested) = project_path(String::from("app/src/generated"), false, manifest, field)
        else {
            panic!("test nested path must be valid");
        };

        let Ok(sibling) = project_path(String::from("app/src-old"), false, manifest, field) else {
            panic!("test sibling path must be valid");
        };

        assert!(paths_overlap(&source, &nested));
        assert!(!paths_overlap(&source, &sibling));
    }
}
