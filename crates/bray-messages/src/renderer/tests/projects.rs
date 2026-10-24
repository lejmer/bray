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
