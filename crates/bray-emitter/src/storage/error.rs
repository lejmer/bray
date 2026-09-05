use std::io;
use std::path::{Path, PathBuf};

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticDocumentParseKind, DiagnosticId, DiagnosticKind,
    DiagnosticNote, DiagnosticNoteKind, DiagnosticRetainedGenerationProblem,
    DiagnosticStorageOperation, SeverityKind,
};

/// Filesystem operation that failed while maintaining managed build storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageOperation {
    /// Inspect a managed file or directory.
    Inspect,
    /// Create a managed directory or ownership lock.
    Create,
    /// Read managed metadata or artifact bytes.
    Read,
    /// Write managed metadata or artifact bytes.
    Write,
    /// Flush managed state to durable storage.
    Flush,
    /// Acquire an ownership or coordination lock.
    Lock,
    /// Atomically publish or retire managed state.
    Rename,
    /// Remove eligible managed state.
    Remove,
}

impl StorageOperation {
    const fn diagnostic(self) -> DiagnosticStorageOperation {
        match self {
            Self::Inspect => DiagnosticStorageOperation::Inspect,
            Self::Create => DiagnosticStorageOperation::Create,
            Self::Read => DiagnosticStorageOperation::Read,
            Self::Write => DiagnosticStorageOperation::Write,
            Self::Flush => DiagnosticStorageOperation::Flush,
            Self::Lock => DiagnosticStorageOperation::Lock,
            Self::Rename => DiagnosticStorageOperation::Rename,
            Self::Remove => DiagnosticStorageOperation::Remove,
        }
    }
}

/// Specific cause of a managed-storage failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageErrorKind {
    /// Retained artifact length differs from its manifest.
    ArtifactLength {
        /// Recorded length.
        expected: u64,
        /// Observed length.
        actual: u64,
    },
    /// Retained artifact content differs from its manifest digest.
    ArtifactDigest {
        /// Recorded digest.
        expected: bray_codegen::ArtifactDigest,
        /// Observed digest.
        actual: bray_codegen::ArtifactDigest,
    },
    /// A retained generation violates its recorded contract.
    Generation(DiagnosticRetainedGenerationProblem),
    /// Managed metadata cannot be decoded using its required schema.
    Metadata {
        /// Parser failure category.
        cause: DiagnosticDocumentParseKind,
        /// Parser line, when supplied and representable.
        line: Option<u64>,
        /// Parser column, when supplied and representable.
        column: Option<u64>,
    },
    /// The metadata schema revision differs from the supported revision.
    Revision {
        /// Required revision.
        expected: u32,
        /// Recorded revision.
        actual: u32,
    },
    /// A filesystem operation failed with this operating-system cause.
    Io {
        /// Operation attempted on the affected path.
        operation: StorageOperation,
        /// Exact portable operating-system error category.
        cause: io::ErrorKind,
    },
    /// A managed path contains traversal, a link, or an unexpected file type.
    UnsafePath,
    /// The requested retained state no longer exists.
    Unavailable,
    /// Maintenance stopped at a cancellation boundary.
    Cancelled,
}

/// A managed-storage failure with the affected path and specific cause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageError {
    path: PathBuf,
    kind: StorageErrorKind,
}

impl StorageError {
    pub(crate) fn new(path: &Path, kind: StorageErrorKind) -> Self {
        Self {
            path: path.to_owned(),
            kind,
        }
    }

    pub(crate) fn io(path: &Path, operation: StorageOperation, error: io::Error) -> Self {
        Self::new(
            path,
            StorageErrorKind::Io {
                operation,
                cause: error.kind(),
            },
        )
    }

    pub(crate) fn json(path: &Path, error: serde_json::Error) -> Self {
        Self::new(
            path,
            StorageErrorKind::Metadata {
                cause: error.classify().into(),
                line: u64::try_from(error.line()).ok(),
                column: u64::try_from(error.column()).ok(),
            },
        )
    }

    pub(crate) fn content(
        path: &Path,
        error: crate::artifact::content::ContentValidationError,
    ) -> Self {
        use crate::artifact::content::ContentValidationError;

        match error {
            ContentValidationError::Cancelled => Self::new(path, StorageErrorKind::Cancelled),
            ContentValidationError::Read(cause) => {
                Self::io(path, StorageOperation::Read, cause.into())
            }
            ContentValidationError::LengthOverflow => Self::io(
                path,
                StorageOperation::Read,
                io::ErrorKind::FileTooLarge.into(),
            ),
            ContentValidationError::LengthMismatch { expected, actual } => {
                Self::new(path, StorageErrorKind::ArtifactLength { expected, actual })
            }
            ContentValidationError::DigestMismatch { expected, actual } => {
                Self::new(path, StorageErrorKind::ArtifactDigest { expected, actual })
            }
            ContentValidationError::DigestConstruction => Self::new(
                path,
                StorageErrorKind::Generation(DiagnosticRetainedGenerationProblem::ArtifactContract),
            ),
        }
    }

    /// Returns the affected file or directory.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the specific failure cause.
    pub const fn kind(&self) -> &StorageErrorKind {
        &self.kind
    }

    /// Converts the failure to a locale-neutral diagnostic without losing its path or cause.
    pub fn into_diagnostic(self, id: DiagnosticId, severity: SeverityKind) -> Diagnostic {
        let kind = match self.kind {
            StorageErrorKind::ArtifactLength { .. } => {
                DiagnosticKind::RetainedArtifactLengthMismatch
            }
            StorageErrorKind::ArtifactDigest { .. } => {
                DiagnosticKind::RetainedArtifactDigestMismatch
            }
            StorageErrorKind::Generation(_) => DiagnosticKind::RetainedGenerationInvalid,
            StorageErrorKind::Metadata { .. } => DiagnosticKind::BuildStorageMetadataInvalid,
            StorageErrorKind::Revision { .. } => DiagnosticKind::BuildStorageRevisionMismatch,
            StorageErrorKind::Io { .. } => DiagnosticKind::BuildStorageIoFailed,
            StorageErrorKind::UnsafePath => DiagnosticKind::BuildStorageUnsafePath,
            StorageErrorKind::Unavailable => DiagnosticKind::RetainedBuildStateUnavailable,
            StorageErrorKind::Cancelled => DiagnosticKind::BuildStorageCancelled,
        };

        let diagnostic =
            Diagnostic::new(id, kind, severity).with_arg(DiagnosticArg::file_path(self.path));

        match self.kind {
            StorageErrorKind::ArtifactLength { expected, actual } => diagnostic
                .with_arg(DiagnosticArg::expected_byte_count(expected))
                .with_arg(DiagnosticArg::actual_byte_count(actual)),
            StorageErrorKind::ArtifactDigest { expected, actual } => diagnostic
                .with_arg(DiagnosticArg::expected_artifact_digest(
                    expected.diagnostic_digest(),
                ))
                .with_arg(DiagnosticArg::actual_artifact_digest(
                    actual.diagnostic_digest(),
                )),
            StorageErrorKind::Generation(problem) => {
                diagnostic.with_arg(DiagnosticArg::retained_generation_problem(problem))
            }
            StorageErrorKind::Metadata {
                cause,
                line,
                column,
            } => {
                let diagnostic = diagnostic.with_arg(DiagnosticArg::document_parse_kind(cause));

                if let (Some(line), Some(column)) = (line, column) {
                    diagnostic.with_note(
                        DiagnosticNote::new(DiagnosticNoteKind::DocumentFailureLocation)
                            .with_arg(DiagnosticArg::document_line(line))
                            .with_arg(DiagnosticArg::document_column(column)),
                    )
                } else {
                    diagnostic
                }
            }
            StorageErrorKind::Revision { expected, actual } => diagnostic
                .with_arg(DiagnosticArg::expected_revision(u64::from(expected)))
                .with_arg(DiagnosticArg::actual_revision(u64::from(actual))),
            StorageErrorKind::Io { operation, cause } => diagnostic
                .with_arg(DiagnosticArg::storage_operation(operation.diagnostic()))
                .with_arg(DiagnosticArg::io_error_kind(cause.into())),
            StorageErrorKind::Unavailable => diagnostic.with_note(DiagnosticNote::new(
                DiagnosticNoteKind::RebuildRetainedProduct,
            )),
            StorageErrorKind::UnsafePath | StorageErrorKind::Cancelled => diagnostic,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bray_diagnostics::{
        DiagnosticBag, DiagnosticDocumentParseKind, DiagnosticId, DiagnosticKind,
        DiagnosticRetainedGenerationProblem, SeverityKind,
    };
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::{StorageError, StorageErrorKind, StorageOperation};

    fn diagnostic(kind: StorageErrorKind) -> DiagnosticBag {
        DiagnosticBag::single(
            StorageError::new(Path::new("managed-state"), kind)
                .into_diagnostic(DiagnosticId::new(0), SeverityKind::Error),
        )
    }

    #[test]
    fn storage_failures_publish_each_exact_goal_state_diagnostic() {
        let io = diagnostic(StorageErrorKind::Io {
            operation: StorageOperation::Read,
            cause: std::io::ErrorKind::NotFound,
        });

        assert_goal_state_diagnostic_kind(&io, DiagnosticKind::BuildStorageIoFailed);

        let unsafe_path = diagnostic(StorageErrorKind::UnsafePath);

        assert_goal_state_diagnostic_kind(&unsafe_path, DiagnosticKind::BuildStorageUnsafePath);

        let unavailable = diagnostic(StorageErrorKind::Unavailable);

        assert_goal_state_diagnostic_kind(
            &unavailable,
            DiagnosticKind::RetainedBuildStateUnavailable,
        );

        let cancelled = diagnostic(StorageErrorKind::Cancelled);

        assert_goal_state_diagnostic_kind(&cancelled, DiagnosticKind::BuildStorageCancelled);

        let metadata = diagnostic(StorageErrorKind::Metadata {
            cause: DiagnosticDocumentParseKind::Schema,
            line: None,
            column: None,
        });

        assert_goal_state_diagnostic_kind(&metadata, DiagnosticKind::BuildStorageMetadataInvalid);

        let revision = diagnostic(StorageErrorKind::Revision {
            expected: 1,
            actual: 2,
        });

        assert_goal_state_diagnostic_kind(&revision, DiagnosticKind::BuildStorageRevisionMismatch);

        let generation = diagnostic(StorageErrorKind::Generation(
            DiagnosticRetainedGenerationProblem::ArtifactContract,
        ));

        assert_goal_state_diagnostic_kind(&generation, DiagnosticKind::RetainedGenerationInvalid);

        let length = diagnostic(StorageErrorKind::ArtifactLength {
            expected: 1,
            actual: 2,
        });

        assert_goal_state_diagnostic_kind(&length, DiagnosticKind::RetainedArtifactLengthMismatch);
    }
}
