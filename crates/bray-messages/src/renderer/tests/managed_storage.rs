use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm,
    DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind, DiagnosticNote, DiagnosticNoteKind,
    SeverityKind,
};

use crate::RenderedDiagnosticNoteKind;
use crate::renderer::DiagnosticRenderer;

#[test]
fn renderer_preserves_managed_storage_failure_context() {
    use bray_diagnostics::{
        DiagnosticDocumentParseKind, DiagnosticRetainedGenerationProblem,
        DiagnosticStorageOperation,
    };

    let base = |kind| {
        Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error)
            .with_arg(DiagnosticArg::file_path("build/app"))
    };

    let cases = [
        (
            base(DiagnosticKind::BuildStorageIoFailed)
                .with_arg(DiagnosticArg::storage_operation(
                    DiagnosticStorageOperation::Remove,
                ))
                .with_arg(DiagnosticArg::io_error_kind(
                    DiagnosticIoErrorKind::PermissionDenied,
                )),
            "Cannot remove managed build state at build/app: permission denied.",
        ),
        (
            base(DiagnosticKind::BuildStorageUnsafePath),
            "Managed build state at build/app contains a link, traversal, or an unexpected file type. Cleanup left it untouched.",
        ),
        (
            base(DiagnosticKind::RetainedBuildStateUnavailable),
            "Retained build state at build/app is unavailable because its retained files no longer exist.",
        ),
        (
            base(DiagnosticKind::BuildStorageCancelled),
            "Build-storage maintenance at build/app was cancelled. A subsequent operation can resume cleanup.",
        ),
        (
            base(DiagnosticKind::BuildStorageMetadataInvalid).with_arg(
                DiagnosticArg::document_parse_kind(DiagnosticDocumentParseKind::Syntax),
            ),
            "Cannot decode managed build metadata at build/app: found invalid JSON syntax",
        ),
        (
            base(DiagnosticKind::BuildStorageRevisionMismatch)
                .with_arg(DiagnosticArg::expected_revision(1))
                .with_arg(DiagnosticArg::actual_revision(7)),
            "Managed build metadata at build/app uses revision 7, but this compiler requires revision 1",
        ),
        (
            base(DiagnosticKind::RetainedGenerationInvalid).with_arg(
                DiagnosticArg::retained_generation_problem(
                    DiagnosticRetainedGenerationProblem::ProductIdentity,
                ),
            ),
            "Retained product state at build/app failed validation: the product manifest names a different package or product",
        ),
        (
            base(DiagnosticKind::RetainedArtifactLengthMismatch)
                .with_arg(DiagnosticArg::expected_byte_count(7))
                .with_arg(DiagnosticArg::actual_byte_count(3)),
            "Retained build file build/app contains 3 bytes, but its manifest requires 7 bytes",
        ),
    ];

    for (diagnostic, expected) in cases {
        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(rendered.message(), expected);
        assert!(rendered.notes().is_empty());
    }

    let digest = base(DiagnosticKind::RetainedArtifactDigestMismatch)
        .with_arg(DiagnosticArg::expected_artifact_digest(
            DiagnosticArtifactDigest::new(DiagnosticArtifactDigestAlgorithm::Sha256, [0; 32]),
        ))
        .with_arg(DiagnosticArg::actual_artifact_digest(
            DiagnosticArtifactDigest::new(DiagnosticArtifactDigestAlgorithm::Sha256, [1; 32]),
        ));

    assert_eq!(
        DiagnosticRenderer::english().render(&digest).message(),
        format!(
            "Retained build file build/app has digest SHA-256 {}, but its manifest requires SHA-256 {}",
            "01".repeat(32),
            "00".repeat(32),
        )
    );
}

#[test]
fn storage_io_failures_preserve_host_categories_in_rendered_output() {
    let cases = [
        (
            std::io::ErrorKind::StorageFull,
            "storage_full",
            "storage is full",
        ),
        (
            std::io::ErrorKind::QuotaExceeded,
            "quota_exceeded",
            "storage quota exceeded",
        ),
        (
            std::io::ErrorKind::FileTooLarge,
            "file_too_large",
            "file exceeds the supported size",
        ),
        (
            std::io::ErrorKind::ReadOnlyFilesystem,
            "read_only_filesystem",
            "filesystem is read-only",
        ),
        (
            std::io::ErrorKind::ResourceBusy,
            "resource_busy",
            "resource is busy",
        ),
        (
            std::io::ErrorKind::ExecutableFileBusy,
            "executable_file_busy",
            "executable file is in use",
        ),
        (
            std::io::ErrorKind::CrossesDevices,
            "crosses_devices",
            "operation crosses filesystem devices",
        ),
        (
            std::io::ErrorKind::TooManyLinks,
            "too_many_links",
            "file has too many hard links",
        ),
        (
            std::io::ErrorKind::InvalidFilename,
            "invalid_filename",
            "filename violates filesystem requirements",
        ),
        (
            std::io::ErrorKind::WriteZero,
            "write_zero",
            "the write made no progress",
        ),
        (
            std::io::ErrorKind::OutOfMemory,
            "out_of_memory",
            "host memory is exhausted",
        ),
        (
            std::io::ErrorKind::NotSeekable,
            "not_seekable",
            "resource does not support seeking",
        ),
        (
            std::io::ErrorKind::DirectoryNotEmpty,
            "directory_not_empty",
            "directory is not empty",
        ),
        (
            std::io::ErrorKind::Unsupported,
            "unsupported",
            "filesystem does not support the operation",
        ),
    ];

    for (cause, key, reason) in cases {
        let cause = DiagnosticIoErrorKind::from(cause);

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::BuildStorageIoFailed,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::file_path("build/application"))
        .with_arg(DiagnosticArg::storage_operation(
            bray_diagnostics::DiagnosticStorageOperation::Write,
        ))
        .with_arg(DiagnosticArg::io_error_kind(cause));

        assert_eq!(cause.as_str(), key);

        assert_eq!(
            DiagnosticRenderer::english().render(&diagnostic).message(),
            format!("Cannot write managed build state at build/application: {reason}.")
        );
    }
}

#[test]
fn renderer_explains_each_retained_metadata_failure() {
    use bray_diagnostics::DiagnosticRetainedGenerationProblem as Problem;

    let cases = [
        (
            Problem::ReferenceRevision {
                expected: 1,
                actual: 3,
            },
            "the publication reference requires revision 1, but records revision 3",
        ),
        (
            Problem::ReferenceIdentity,
            "the publication reference contains a malformed locator or content digest",
        ),
        (
            Problem::ManifestRevision {
                expected: 1,
                actual: 3,
            },
            "the product manifest requires revision 1, but records revision 3",
        ),
        (
            Problem::ArtifactSelection,
            "the requested artifact is absent or appears more than once",
        ),
        (
            Problem::ArtifactContract,
            "an artifact violates its recorded path or identity contract",
        ),
        (
            Problem::DigestAlgorithm,
            "the manifest records an unsupported artifact digest algorithm",
        ),
        (
            Problem::DigestEncoding,
            "the manifest artifact digest is not a lowercase 32-byte hexadecimal value",
        ),
        (
            Problem::LogicalPermission,
            "the manifest permission kind disagrees with its artifact kind",
        ),
        (
            Problem::ReadOnly {
                expected: false,
                actual: true,
            },
            "the manifest records read-only status false, but the file has read-only status true",
        ),
        (
            Problem::UnixMode {
                expected: Some(0o755),
                actual: Some(0o644),
            },
            "the manifest records Unix permissions 0o755, but the file has Unix permissions 0o644",
        ),
        (
            Problem::UnixMode {
                expected: Some(0o755),
                actual: None,
            },
            "the manifest records Unix permissions 0o755, but the file has Unix permissions unavailable on this filesystem",
        ),
    ];

    for (problem, reason) in cases {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::RetainedGenerationInvalid,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::file_path("build/application"))
        .with_arg(DiagnosticArg::retained_generation_problem(problem));

        assert_eq!(
            DiagnosticRenderer::english().render(&diagnostic).message(),
            format!("Retained product state at build/application failed validation: {reason}")
        );
    }
}

#[test]
fn renderer_explains_retained_rebuilds_and_metadata_source_locations() {
    let unavailable = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::RetainedBuildStateUnavailable,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::file_path("build/app"))
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::RebuildRetainedProduct,
    ));

    let rendered = DiagnosticRenderer::english().render(&unavailable);

    assert_eq!(
        rendered.notes()[0].rendered_kind(),
        RenderedDiagnosticNoteKind::Help
    );

    assert_eq!(
        rendered.notes()[0].message(),
        "build the selected product again before requesting a no-build run"
    );

    let metadata = Diagnostic::new(
        DiagnosticId::new(1),
        DiagnosticKind::BuildStorageMetadataInvalid,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::file_path("build/metadata.json"))
    .with_arg(DiagnosticArg::document_parse_kind(
        bray_diagnostics::DiagnosticDocumentParseKind::Schema,
    ))
    .with_note(
        DiagnosticNote::new(DiagnosticNoteKind::DocumentFailureLocation)
            .with_arg(DiagnosticArg::document_line(4))
            .with_arg(DiagnosticArg::document_column(9)),
    );

    let rendered = DiagnosticRenderer::english().render(&metadata);

    assert_eq!(
        rendered.message(),
        "Cannot decode managed build metadata at build/metadata.json: found a value that does not match the required schema"
    );

    assert_eq!(
        rendered.notes()[0].message(),
        "the document parser reported line 4, column 9"
    );
}
