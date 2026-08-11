use std::fs;
use std::path::{Path, PathBuf};

use bray_diagnostics::DiagnosticProjectManifestField;
use crate::{ProjectLoadError, ProjectPath};

#[derive(Clone, Copy)]
enum SourcePathProblem {
    Invalid,
    Symlink,
    NonUtf8,
}

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
            SourcePathProblem::Invalid,
            source_directory.to_path_buf(),
        )
    })?;

    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid_source_root(
            manifest_path,
            if metadata.file_type().is_symlink() {
                SourcePathProblem::Symlink
            } else {
                SourcePathProblem::Invalid
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
            SourcePathProblem::Invalid,
            directory.to_path_buf(),
        )
    })?;

    let mut entries = entries
        .map(|entry| {
            let entry = entry.map_err(|_| {
                invalid_source_root(
                    manifest_path,
                    SourcePathProblem::Invalid,
                    directory.to_path_buf(),
                )
            })?;

            let name = entry.file_name().into_string().map_err(|_| {
                invalid_source_root(
                    manifest_path,
                    SourcePathProblem::NonUtf8,
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
                SourcePathProblem::Invalid,
                entry_path.to_path_buf(),
            )
        })?;

        if metadata.file_type().is_symlink() {
            return Err(invalid_source_root(
                manifest_path,
                SourcePathProblem::Symlink,
                entry_path,
            ));
        }

        let Some(name_path) = ProjectPath::try_new(name.as_str(), false) else {
            return Err(invalid_source_root(
                manifest_path,
                SourcePathProblem::Invalid,
                entry_path,
            ));
        };

        let relative_path = relative_directory.joined(&name_path);

        if metadata.is_dir() {
            collect_directory(&entry_path, &relative_path, manifest_path, sources)?;
        } else if metadata.is_file()
            && entry_path
                .extension()
                .is_some_and(|extension| extension == "bray")
        {
            sources.push(relative_path);
        } else if !metadata.is_file() {
            return Err(invalid_source_root(
                manifest_path,
                SourcePathProblem::Invalid,
                entry_path,
            ));
        }
    }

    Ok(())
}

fn invalid_source_root(
    manifest_path: &Path,
    problem: SourcePathProblem,
    source_path: PathBuf,
) -> ProjectLoadError {
    match problem {
        SourcePathProblem::Invalid => ProjectLoadError::invalid_source_root(
            manifest_path.to_path_buf(),
            DiagnosticProjectManifestField::SourceRootPath,
            source_path,
        ),
        SourcePathProblem::Symlink => ProjectLoadError::source_symlink(
            manifest_path.to_path_buf(),
            DiagnosticProjectManifestField::SourceRootPath,
            source_path,
        ),
        SourcePathProblem::NonUtf8 => ProjectLoadError::non_utf8_source_path(
            manifest_path.to_path_buf(),
            DiagnosticProjectManifestField::SourceRootPath,
            source_path,
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use bray_diagnostics::{DiagnosticBag, DiagnosticId, DiagnosticKind};

    use super::{SourcePathProblem, invalid_source_root};

    #[test]
    fn source_path_failures_keep_their_exact_project_diagnostics() {
        let manifest = Path::new("bray-package.json");

        let symlink = DiagnosticBag::single(
            invalid_source_root(
                manifest,
                SourcePathProblem::Symlink,
                PathBuf::from("src/generated"),
            )
            .into_diagnostic(DiagnosticId::new(0)),
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &symlink,
            DiagnosticKind::ProjectSourceRootContainsSymlink,
        );

        let non_utf8 = DiagnosticBag::single(
            invalid_source_root(
                manifest,
                SourcePathProblem::NonUtf8,
                PathBuf::from("src/non-utf8"),
            )
            .into_diagnostic(DiagnosticId::new(1)),
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &non_utf8,
            DiagnosticKind::ProjectSourceRootContainsNonUtf8Path,
        );
    }
}
