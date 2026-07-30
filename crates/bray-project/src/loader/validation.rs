use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_symbols::PackageIdentity;
use serde::de::DeserializeOwned;

use crate::{ProjectLoadError, ProjectManifestProblem, ProjectPath};

pub(super) const MANIFEST_FORMAT_REVISION: u32 = 1;

pub(super) fn read_manifest<T: DeserializeOwned>(path: &Path) -> Result<T, ProjectLoadError> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| ProjectLoadError::ReadManifest {
            path: path.to_path_buf(),
            kind: error.kind(),
        })?;

    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ProjectLoadError::invalid(
            path.to_path_buf(),
            ProjectManifestProblem::InvalidPath,
            path.display().to_string(),
        ));
    }

    let text = std::fs::read_to_string(path).map_err(|error| ProjectLoadError::ReadManifest {
        path: path.to_path_buf(),
        kind: error.kind(),
    })?;

    serde_json::from_str(&text).map_err(|_| ProjectLoadError::ParseManifest {
        path: path.to_path_buf(),
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
            ProjectLoadError::invalid(
                workspace_manifest_path.to_path_buf(),
                ProjectManifestProblem::InvalidPath,
                package_path.as_str().to_owned(),
            )
        })?;

        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(ProjectLoadError::invalid(
                workspace_manifest_path.to_path_buf(),
                ProjectManifestProblem::InvalidPath,
                package_path.as_str().to_owned(),
            ));
        }
    }

    Ok(())
}

pub(super) fn require_format(format: u32, path: &Path) -> Result<(), ProjectLoadError> {
    if format == MANIFEST_FORMAT_REVISION {
        return Ok(());
    }

    Err(ProjectLoadError::invalid(
        path.to_path_buf(),
        ProjectManifestProblem::UnsupportedFormat,
        format.to_string(),
    ))
}

pub(super) fn project_path(
    value: String,
    allow_workspace_root: bool,
    manifest_path: &Path,
) -> Result<ProjectPath, ProjectLoadError> {
    ProjectPath::try_new(Arc::<str>::from(value.as_str()), allow_workspace_root).ok_or_else(|| {
        ProjectLoadError::invalid(
            manifest_path.to_path_buf(),
            ProjectManifestProblem::InvalidPath,
            value,
        )
    })
}

pub(super) fn local_name(
    value: String,
    manifest_path: &Path,
) -> Result<Arc<str>, ProjectLoadError> {
    if is_local_name(&value) {
        return Ok(value.into());
    }

    Err(ProjectLoadError::invalid(
        manifest_path.to_path_buf(),
        ProjectManifestProblem::InvalidName,
        value,
    ))
}

pub(super) fn package_identity(
    value: String,
    manifest_path: &Path,
) -> Result<PackageIdentity, ProjectLoadError> {
    if !value.split('.').all(is_package_segment) || !value.contains('.') {
        return Err(ProjectLoadError::invalid(
            manifest_path.to_path_buf(),
            ProjectManifestProblem::InvalidName,
            value,
        ));
    }

    PackageIdentity::try_new(Arc::<str>::from(value.as_str())).ok_or_else(|| {
        ProjectLoadError::invalid(
            manifest_path.to_path_buf(),
            ProjectManifestProblem::InvalidName,
            value,
        )
    })
}

pub(super) fn sorted_unique_names(
    values: Vec<String>,
    manifest_path: &Path,
) -> Result<Arc<[Arc<str>]>, ProjectLoadError> {
    let mut names = values
        .into_iter()
        .map(|value| local_name(value, manifest_path))
        .collect::<Result<Vec<_>, _>>()?;

    names.sort_unstable();

    if let Some(pair) = names.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(ProjectLoadError::invalid(
            manifest_path.to_path_buf(),
            ProjectManifestProblem::DuplicateSelection,
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
    !value.is_empty() && is_local_name(value) && !value.contains('_')
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

    use super::{local_name, package_identity, paths_overlap, project_path};

    #[test]
    fn names_have_one_canonical_ascii_spelling() {
        let manifest = Path::new("bray-package.json");

        assert!(local_name(String::from("native-test"), manifest).is_ok());
        assert!(local_name(String::from("Native"), manifest).is_err());
        assert!(local_name(String::from("9native"), manifest).is_err());
        assert!(package_identity(String::from("example.math"), manifest).is_ok());
        assert!(package_identity(String::from("example"), manifest).is_err());
        assert!(package_identity(String::from("example.math_core"), manifest).is_err());
    }

    #[test]
    fn overlap_uses_portable_component_boundaries() {
        let manifest = Path::new("bray-workspace.json");

        let Ok(source) = project_path(String::from("app/src"), false, manifest) else {
            panic!("test source path must be valid");
        };

        let Ok(nested) = project_path(String::from("app/src/generated"), false, manifest) else {
            panic!("test nested path must be valid");
        };

        let Ok(sibling) = project_path(String::from("app/src-old"), false, manifest) else {
            panic!("test sibling path must be valid");
        };

        assert!(paths_overlap(&source, &nested));
        assert!(!paths_overlap(&source, &sibling));
    }
}
