use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticNote, DiagnosticNoteKind,
    DiagnosticProjectManifestField, DiagnosticTargetPredicateValueKind, SeverityKind,
};

use crate::RenderedDiagnosticNoteKind;
use crate::renderer::DiagnosticRenderer;

#[test]
fn renderer_explains_valid_project_initialization_identities() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::ProjectInitializationIdentityInvalid,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::referenced_name("Invalid Package"))
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::PackageIdentityMustBeValid,
    ));

    let rendered = DiagnosticRenderer::english().render(&diagnostic);

    assert_eq!(
        rendered.message(),
        "cannot use 'Invalid Package' as a Bray package identity"
    );

    let [note] = rendered.notes() else {
        panic!("expected package identity help: {rendered:?}");
    };

    assert_eq!(note.rendered_kind(), RenderedDiagnosticNoteKind::Help);

    assert_eq!(
        note.message(),
        concat!(
            "use a non-reserved lowercase name or dot-separated names, starting each name ",
            "with a letter and using only letters, digits, underscores, or hyphens",
        )
    );
}

#[test]
fn renderer_preserves_both_target_predicate_failure_shapes() {
    let unknown = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::ProjectManifestUnknownTargetPredicateProperty,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::file_path("bray-package.json"))
    .with_arg(DiagnosticArg::project_manifest_field(
        DiagnosticProjectManifestField::TargetPredicate,
    ))
    .with_arg(DiagnosticArg::referenced_name("target.unknown"));

    let mismatch = Diagnostic::new(
        DiagnosticId::new(1),
        DiagnosticKind::ProjectManifestTargetPredicateValueKindMismatch,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::file_path("bray-package.json"))
    .with_arg(DiagnosticArg::project_manifest_field(
        DiagnosticProjectManifestField::TargetPredicate,
    ))
    .with_arg(DiagnosticArg::referenced_name("target.pointer.BITS"))
    .with_arg(DiagnosticArg::expected_target_predicate_value_kind(
        DiagnosticTargetPredicateValueKind::UnsignedInteger,
    ))
    .with_arg(DiagnosticArg::actual_target_predicate_value_kind(
        DiagnosticTargetPredicateValueKind::String,
    ));

    let renderer = DiagnosticRenderer::english();

    assert_eq!(
        renderer.render(&unknown).message(),
        "target predicate property 'target.unknown' is not defined for target predicate in bray-package.json"
    );

    assert_eq!(
        renderer.render(&mismatch).message(),
        "target predicate property 'target.pointer.BITS' in target predicate of bray-package.json accepts unsigned integer values, but received a string value"
    );
}

#[test]
fn test_host_file_failures_preserve_the_path_and_specific_cause() {
    use bray_diagnostics::DiagnosticProjectCommandFailure as Failure;

    for (failure, expected) in [
        (
            Failure::DuplicateTestCatalogEntry {
                path: "tests.braytest".into(),
                identity: "sample/tests::checks.works".into(),
            },
            "test 'sample/tests::checks.works' appears more than once in the test product data at tests.braytest",
        ),
        (
            Failure::TestCatalogResultMismatch {
                path: "tests.braytest".into(),
                identity: "sample/tests::checks.works".into(),
            },
            "test 'sample/tests::checks.works' has a result type that disagrees with its recoverable error type in the test product data at tests.braytest",
        ),
        (
            Failure::MalformedTestCatalog {
                path: "tests.braytest".into(),
            },
            "the test catalog at tests.braytest has malformed contents",
        ),
        (
            Failure::UnsupportedTestCatalogVersion {
                path: "tests.braytest".into(),
                actual: 2,
                supported: 1,
            },
            "the test catalog at tests.braytest uses format version 2, but this compiler supports version 1",
        ),
        (
            Failure::TestCatalogCapacityExceeded {
                path: "tests.braytest".into(),
            },
            "the test catalog at tests.braytest exceeds the supported size limits",
        ),
        (
            Failure::TestHostChanged {
                path: "tests.exe".into(),
            },
            "the test host file at tests.exe changed before execution",
        ),
    ] {
        let diagnostic = failure.diagnostic(DiagnosticId::new(0));

        assert_eq!(
            DiagnosticRenderer::english().render(&diagnostic).message(),
            format!("project command failed: {expected}")
        );
    }
}

#[test]
fn generator_failures_identify_the_generator_and_preserve_each_specific_cause() {
    use bray_diagnostics::{DiagnosticIoErrorKind, DiagnosticProjectGeneratorFailure as Failure};

    for (failure, expected) in [
        (
            Failure::UnknownReference {
                name: "bindings/types.bray".into(),
            },
            "'bindings/types.bray' does not name a declared generator or output",
        ),
        (
            Failure::ToolNotExecutable {
                tool: "sample/tool/native".into(),
            },
            "tool 'sample/tool/native' must name a declared executable product with executable output",
        ),
        (
            Failure::ToolTargetMismatch {
                target: "other".into(),
                host_target: Some("x86_64-unknown-linux-gnu".into()),
            },
            "tool target 'other' does not match compiler host target 'x86_64-unknown-linux-gnu'",
        ),
        (
            Failure::ToolTargetMismatch {
                target: "other".into(),
                host_target: None,
            },
            "the compiler host cannot execute tool target 'other'",
        ),
        (
            Failure::InvalidEnvironment {
                name: "BAD=NAME".into(),
            },
            "environment variable 'BAD=NAME' has a name or value that cannot be passed to a process",
        ),
        (
            Failure::OutputCollision {
                path: "types.bray".into(),
            },
            "output 'types.bray' collides with another declared or written output",
        ),
        (
            Failure::InvalidOutput {
                path: "../types.bray".into(),
            },
            "output '../types.bray' must be a portable relative file path with the exact .bray extension",
        ),
        (
            Failure::UndeclaredOutput {
                path: "extra.bray".into(),
            },
            "output 'extra.bray' is not declared",
        ),
        (
            Failure::MissingOutput {
                path: "types.bray".into(),
            },
            "declared output 'types.bray' was not produced",
        ),
        (
            Failure::NonUtf8Output {
                path: "types.bray".into(),
            },
            "output 'types.bray' is not UTF-8 source text",
        ),
        (
            Failure::InputNotRegular {
                path: "schema.json".into(),
            },
            "input 'schema.json' must be a regular file beneath its package with no symbolic links",
        ),
        (
            Failure::InputIo {
                path: "schema.json".into(),
                kind: DiagnosticIoErrorKind::NotFound,
            },
            "cannot read input 'schema.json': not found",
        ),
        (
            Failure::DependencyCycle,
            "product, host-tool, and generated-source dependencies form a cycle",
        ),
        (
            Failure::ExecutorUnavailable,
            "the host has not supplied a sandbox executor",
        ),
        (
            Failure::ToolUnavailable {
                tool: "sample/tool/native".into(),
            },
            "the host has not supplied the exact tool artifact 'sample/tool/native'",
        ),
        (
            Failure::Cancelled,
            "generation was cancelled before publication",
        ),
        (
            Failure::SandboxUnavailable,
            "the executor cannot enforce the required filesystem and network isolation",
        ),
        (
            Failure::AccessDenied {
                resource: "network".into(),
            },
            "the sandbox denied access to 'network'",
        ),
        (
            Failure::ExecutionFailed { code: Some(7) },
            "the generator exited with code 7",
        ),
        (
            Failure::ExecutionFailed { code: None },
            "the generator was terminated without an exit code",
        ),
        (
            Failure::ExecutorIo {
                kind: DiagnosticIoErrorKind::NotFound,
            },
            "the sandbox executor failed: not found",
        ),
    ] {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::ProjectGeneratorFailed,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::file_path("bray-package.json"))
        .with_arg(DiagnosticArg::referenced_name("sample/bindings"))
        .with_arg(DiagnosticArg::project_generator_failure(failure));

        assert_eq!(
            DiagnosticRenderer::english().render(&diagnostic).message(),
            format!("source generator 'sample/bindings' in bray-package.json failed: {expected}")
        );
    }
}
