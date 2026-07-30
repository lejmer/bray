use std::fs;
use std::path::{Path, PathBuf};

use crate::{ProjectLoadError, ProjectManifestProblem, ProjectPath};

pub(super) fn collect_sources(
    workspace_root: &Path,
    package_path: &ProjectPath,
    source_path: &ProjectPath,
    manifest_path: &Path,
) -> Result<Box<[ProjectPath]>, ProjectLoadError> {
    let workspace_source_path = package_path.joined(source_path);
    let source_directory = workspace_source_path.beneath(workspace_root);

    let metadata = fs::symlink_metadata(&source_directory).map_err(|_| {
        invalid_source_root(
            manifest_path,
            ProjectManifestProblem::InvalidSourceRoot,
            source_directory.to_path_buf(),
        )
    })?;

    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid_source_root(
            manifest_path,
            if metadata.file_type().is_symlink() {
                ProjectManifestProblem::SourceSymlink
            } else {
                ProjectManifestProblem::InvalidSourceRoot
            },
            source_directory,
        ));
    }

    let mut sources = Vec::new();

    collect_directory(
        &source_directory,
        &workspace_source_path,
        manifest_path,
        &mut sources,
    )?;

    sources.sort_unstable();

    Ok(sources.into_boxed_slice())
}

fn collect_directory(
    directory: &Path,
    relative_directory: &ProjectPath,
    manifest_path: &Path,
    sources: &mut Vec<ProjectPath>,
) -> Result<(), ProjectLoadError> {
    let entries = fs::read_dir(directory).map_err(|_| {
        invalid_source_root(
            manifest_path,
            ProjectManifestProblem::InvalidSourceRoot,
            directory.to_path_buf(),
        )
    })?;

    let mut entries = entries
        .map(|entry| {
            let entry = entry.map_err(|_| {
                invalid_source_root(
                    manifest_path,
                    ProjectManifestProblem::InvalidSourceRoot,
                    directory.to_path_buf(),
                )
            })?;

            let name = entry.file_name().into_string().map_err(|_| {
                invalid_source_root(
                    manifest_path,
                    ProjectManifestProblem::NonUtf8SourcePath,
                    entry.path(),
                )
            })?;

            Ok((name, entry))
        })
        .collect::<Result<Vec<_>, ProjectLoadError>>()?;

    entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));

    for (name, entry) in entries {
        let entry_path = entry.path();

        let metadata = fs::symlink_metadata(&entry_path).map_err(|_| {
            invalid_source_root(
                manifest_path,
                ProjectManifestProblem::InvalidSourceRoot,
                entry_path.to_path_buf(),
            )
        })?;

        if metadata.file_type().is_symlink() {
            return Err(invalid_source_root(
                manifest_path,
                ProjectManifestProblem::SourceSymlink,
                entry_path,
            ));
        }

        let Some(name_path) = ProjectPath::try_new(name.as_str(), false) else {
            return Err(invalid_source_root(
                manifest_path,
                ProjectManifestProblem::InvalidSourceRoot,
                entry_path,
            ));
        };

        let relative_path = relative_directory.joined(&name_path);

        if metadata.is_dir() {
            collect_directory(&entry_path, &relative_path, manifest_path, sources)?;
        } else if metadata.is_file() && entry_path.extension().is_some_and(|extension| extension == "bray") {
            sources.push(relative_path);
        } else if !metadata.is_file() {
            return Err(invalid_source_root(
                manifest_path,
                ProjectManifestProblem::InvalidSourceRoot,
                entry_path,
            ));
        }
    }

    Ok(())
}

fn invalid_source_root(
    manifest_path: &Path,
    problem: ProjectManifestProblem,
    source_path: PathBuf,
) -> ProjectLoadError {
    ProjectLoadError::invalid(
        manifest_path.to_path_buf(),
        problem,
        source_path.display().to_string(),
    )
}
