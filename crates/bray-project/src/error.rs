use std::io::ErrorKind;
use std::path::PathBuf;

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind, SeverityKind,
};

/// Exact validation failure in a syntactically valid project manifest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectManifestProblem {
    /// The manifest format revision is not supported.
    UnsupportedFormat,
    /// A portable project path is malformed.
    InvalidPath,
    /// A package, product, feature, source-root, or target name is malformed.
    InvalidName,
    /// An ordinary project package claims the reserved standard library namespace.
    ReservedPackageIdentity,
    /// A standard library project package is outside the reserved namespace.
    StandardLibraryPackageIdentityRequired,
    /// A required manifest collection is empty.
    MissingSelection,
    /// A canonical name, path, package, product, target, feature, or output is repeated.
    DuplicateSelection,
    /// No root package is declared.
    MissingRootPackage,
    /// A workspace feature selection is not declared by its package.
    UndeclaredFeature,
    /// A product selects a source root not declared by its package.
    UnknownSourceRoot,
    /// A product selects a target not declared by its workspace.
    UnknownTarget,
    /// A dependency selects a package not declared by its workspace.
    UnknownDependencyPackage,
    /// A dependency selects a product not declared by its package.
    UnknownDependencyProduct,
    /// A dependency selects a non-library product.
    DependencyProductNotLibrary,
    /// A declared source root cannot be read as a project-owned directory.
    InvalidSourceRoot,
    /// A declared source tree contains a symbolic link.
    SourceSymlink,
    /// A declared source tree contains a non-UTF-8 path.
    NonUtf8SourcePath,
    /// Package dependencies form a cycle.
    DependencyCycle,
}

/// A structured project graph loading failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectLoadError {
    /// A required workspace or package manifest could not be read.
    ReadManifest {
        /// Manifest path.
        path: PathBuf,
        /// Structured host I/O category.
        kind: ErrorKind,
    },
    /// A required manifest is not valid UTF-8 JSON matching the Bray schema.
    ParseManifest {
        /// Manifest path.
        path: PathBuf,
    },
    /// A manifest value violates a Bray project invariant.
    InvalidManifest {
        /// Manifest path.
        path: PathBuf,
        /// Exact locale-neutral validation category.
        problem: ProjectManifestProblem,
        /// Relevant canonical name or serialized value.
        value: String,
    },
}

impl ProjectLoadError {
    pub(crate) fn invalid(
        path: PathBuf,
        problem: ProjectManifestProblem,
        value: impl Into<String>,
    ) -> Self {
        Self::InvalidManifest {
            path,
            problem,
            value: value.into(),
        }
    }

    /// Converts this project-boundary failure into a locale-neutral diagnostic.
    pub fn into_diagnostic(self, id: DiagnosticId) -> Diagnostic {
        match self {
            Self::ReadManifest { path, kind } => Diagnostic::new(
                id,
                DiagnosticKind::ProjectManifestReadFailed,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::file_path(path))
            .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
                kind,
            ))),
            Self::ParseManifest { path } => Diagnostic::new(
                id,
                DiagnosticKind::ProjectManifestParseFailed,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::file_path(path)),
            Self::InvalidManifest {
                path,
                problem,
                value,
            } => Diagnostic::new(id, diagnostic_kind(problem), SeverityKind::Error)
                .with_arg(DiagnosticArg::file_path(path))
                .with_arg(DiagnosticArg::referenced_name(value)),
        }
    }
}

const fn diagnostic_kind(problem: ProjectManifestProblem) -> DiagnosticKind {
    match problem {
        ProjectManifestProblem::UnknownDependencyPackage => {
            DiagnosticKind::ProjectDependencyPackageUnknown
        }
        ProjectManifestProblem::UnknownDependencyProduct
        | ProjectManifestProblem::DependencyProductNotLibrary => {
            DiagnosticKind::ProjectDependencyProductInvalid
        }
        ProjectManifestProblem::DependencyCycle => DiagnosticKind::ProjectDependencyCycle,
        ProjectManifestProblem::ReservedPackageIdentity => {
            DiagnosticKind::ProjectPackageIdentityReserved
        }
        ProjectManifestProblem::StandardLibraryPackageIdentityRequired => {
            DiagnosticKind::ProjectStandardLibraryPackageIdentityRequired
        }
        ProjectManifestProblem::InvalidSourceRoot
        | ProjectManifestProblem::SourceSymlink
        | ProjectManifestProblem::NonUtf8SourcePath => DiagnosticKind::ProjectSourceRootInvalid,
        ProjectManifestProblem::DuplicateSelection => {
            DiagnosticKind::ProjectManifestDuplicateSelection
        }
        ProjectManifestProblem::UnsupportedFormat
        | ProjectManifestProblem::InvalidPath
        | ProjectManifestProblem::InvalidName
        | ProjectManifestProblem::MissingSelection
        | ProjectManifestProblem::MissingRootPackage
        | ProjectManifestProblem::UndeclaredFeature
        | ProjectManifestProblem::UnknownSourceRoot
        | ProjectManifestProblem::UnknownTarget => DiagnosticKind::ProjectManifestInvalid,
    }
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;
    use std::path::PathBuf;

    use bray_diagnostics::{DiagnosticId, DiagnosticKind};

    use super::{ProjectLoadError, ProjectManifestProblem};

    #[test]
    fn project_load_errors_preserve_their_diagnostic_categories() {
        let manifest = PathBuf::from("bray-workspace.json");

        let cases = [
            (
                ProjectLoadError::ReadManifest {
                    path: manifest.clone(),
                    kind: ErrorKind::NotFound,
                },
                DiagnosticKind::ProjectManifestReadFailed,
            ),
            (
                ProjectLoadError::ParseManifest {
                    path: manifest.clone(),
                },
                DiagnosticKind::ProjectManifestParseFailed,
            ),
            (
                ProjectLoadError::invalid(
                    manifest.clone(),
                    ProjectManifestProblem::UnsupportedFormat,
                    "2",
                ),
                DiagnosticKind::ProjectManifestInvalid,
            ),
            (
                ProjectLoadError::invalid(
                    manifest.clone(),
                    ProjectManifestProblem::DuplicateSelection,
                    "app",
                ),
                DiagnosticKind::ProjectManifestDuplicateSelection,
            ),
            (
                ProjectLoadError::invalid(
                    manifest.clone(),
                    ProjectManifestProblem::InvalidSourceRoot,
                    "src",
                ),
                DiagnosticKind::ProjectSourceRootInvalid,
            ),
            (
                ProjectLoadError::invalid(
                    manifest.clone(),
                    ProjectManifestProblem::UnknownDependencyPackage,
                    "vendor.math",
                ),
                DiagnosticKind::ProjectDependencyPackageUnknown,
            ),
            (
                ProjectLoadError::invalid(manifest, ProjectManifestProblem::DependencyCycle, "app"),
                DiagnosticKind::ProjectDependencyCycle,
            ),
        ];

        for (index, (error, expected)) in cases.into_iter().enumerate() {
            let diagnostic = error.into_diagnostic(DiagnosticId::from_index(index));

            assert_eq!(diagnostic.kind(), expected);
        }
    }
}
