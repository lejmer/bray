use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm,
    DiagnosticArtifactKind, DiagnosticCheckerFailure, DiagnosticEmissionEvaluationFailure,
    DiagnosticEmissionFailure, DiagnosticExternalToolExit, DiagnosticId, DiagnosticIoErrorKind,
    DiagnosticKind, DiagnosticNote, DiagnosticNoteKind, DiagnosticOutputSink,
    DiagnosticRuntimeAbiVersion, DiagnosticRuntimeArtifactProblem, SeverityKind,
};

use crate::RenderedDiagnosticNoteKind;
use crate::catalog::{INTERNAL_COMPILER_ERROR, forbidden_ordinary_diagnostic_term};
use crate::renderer::DiagnosticRenderer;

#[test]
fn emission_checker_failures_name_the_product_type_and_reporting_action() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(8),
        DiagnosticKind::EmissionFailed,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::actual_product_identity(
        "example.application",
    ))
    .with_arg(DiagnosticArg::target_triple("x86_64-unknown-linux-gnu"))
    .with_arg(DiagnosticArg::emission_failure(
        DiagnosticEmissionFailure::Evaluation(DiagnosticEmissionEvaluationFailure::Checker(
            DiagnosticCheckerFailure::CompilerKnownRepresentationUnavailable("ScalarU32"),
        )),
    ))
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::ReportCompilerDefect,
    ));

    let rendered = DiagnosticRenderer::english().render(&diagnostic);

    assert!(rendered.message().contains("example.application"));
    assert!(rendered.message().contains("x86_64-unknown-linux-gnu"));
    assert!(rendered.message().contains("`u32`"));
    assert!(!rendered.message().contains("ScalarU32"));
    assert_eq!(rendered.notes().len(), 1);

    assert!(
        rendered.notes()[0]
            .message()
            .contains("report this compiler defect")
    );

    assert_eq!(forbidden_ordinary_diagnostic_term(rendered.message()), None);
}

#[test]
fn native_artifact_read_failures_identify_output_and_io_cause() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(10),
        DiagnosticKind::EmissionFailed,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::actual_product_identity("example.math"))
    .with_arg(DiagnosticArg::target_triple("x86_64-pc-windows-msvc"))
    .with_arg(DiagnosticArg::emission_failure(
        DiagnosticEmissionFailure::NativeRead {
            path: std::path::PathBuf::from("build/math.lib"),
            kind: bray_diagnostics::DiagnosticIoErrorKind::PermissionDenied,
        },
    ));

    let rendered = DiagnosticRenderer::english().render(&diagnostic);

    assert!(rendered.message().contains("example.math"));
    assert!(rendered.message().contains("build/math.lib"));
    assert!(rendered.message().contains("temporary compiler output"));
    assert!(rendered.message().contains("permission denied"));
    assert_eq!(forbidden_ordinary_diagnostic_term(rendered.message()), None);
    assert!(!rendered.message().contains(INTERNAL_COMPILER_ERROR));

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(11),
        DiagnosticKind::CodegenArtifactReadFailed,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::target_triple("x86_64-pc-windows-msvc"))
    .with_arg(DiagnosticArg::codegen_backend_identity("llvm"))
    .with_arg(DiagnosticArg::artifact_kind(
        bray_diagnostics::DiagnosticArtifactKind::BackendBitcode,
    ))
    .with_arg(DiagnosticArg::io_error_kind(
        bray_diagnostics::DiagnosticIoErrorKind::PermissionDenied,
    ));

    let rendered = DiagnosticRenderer::english().render(&diagnostic);

    assert!(rendered.message().contains("x86_64-pc-windows-msvc"));
    assert!(rendered.message().contains("permission denied"));
    assert!(rendered.message().contains("bitcode"));
    assert!(!rendered.message().contains("LLVM bitcode"));
    assert_eq!(forbidden_ordinary_diagnostic_term(rendered.message()), None);
    assert!(!rendered.message().contains(INTERNAL_COMPILER_ERROR));
}

#[test]
fn backend_support_program_failures_render_captured_output_as_text() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(10),
        DiagnosticKind::CodegenBackendToolExited,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::target_triple("x86_64-pc-windows-msvc"))
    .with_arg(DiagnosticArg::codegen_backend_identity("llvm"))
    .with_arg(DiagnosticArg::file_path("C:/toolchain/opt.exe"))
    .with_arg(DiagnosticArg::external_tool_exit(
        DiagnosticExternalToolExit::new(Some(1), &[], b"permission denied\n"),
    ));

    let rendered = DiagnosticRenderer::english().render(&diagnostic);

    assert!(rendered.message().contains("C:/toolchain/opt.exe"));
    assert!(rendered.message().contains("exit code 1"));
    assert!(rendered.message().contains("permission denied"));
    assert!(!rendered.message().contains("[112, 101, 114"));
}

#[test]
fn renderer_localizes_structured_emission_diagnostics() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::EmissionArtifactWriteFailed,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::artifact_kind(
        DiagnosticArtifactKind::PackageInterface,
    ))
    .with_arg(DiagnosticArg::artifact_ordinal(0))
    .with_arg(DiagnosticArg::output_sink(DiagnosticOutputSink::Memory(
        "host.output".to_owned(),
    )))
    .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::Other));

    let rendered = DiagnosticRenderer::english().render(&diagnostic);

    assert_eq!(
        rendered.message(),
        "could not write package interface artifact 0 output memory collector 'host.output': other I/O error"
    );
}

#[test]
fn renderer_localizes_artifact_digest_mismatch_context() {
    let expected =
        DiagnosticArtifactDigest::new(DiagnosticArtifactDigestAlgorithm::Sha256, [0_u8; 32]);

    let actual =
        DiagnosticArtifactDigest::new(DiagnosticArtifactDigestAlgorithm::Sha256, [1_u8; 32]);

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::EmissionArtifactDigestMismatch,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::artifact_kind(
        DiagnosticArtifactKind::PackageInterface,
    ))
    .with_arg(DiagnosticArg::artifact_ordinal(0))
    .with_arg(DiagnosticArg::expected_artifact_digest(expected))
    .with_arg(DiagnosticArg::actual_artifact_digest(actual));

    let rendered = DiagnosticRenderer::english().render(&diagnostic);

    let expected = format!(
        "package interface artifact 0 declared digest SHA-256 {}, but content digest was SHA-256 {}",
        "00".repeat(32),
        "01".repeat(32)
    );

    assert_eq!(rendered.message(), expected);
}

#[test]
fn renderer_localizes_standard_library_artifact_and_abi_failures() {
    let length = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::StandardLibraryArtifactLengthMismatch,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::file_path("targets/test/1.0/libstd.a"))
    .with_arg(DiagnosticArg::actual_byte_count(7))
    .with_arg(DiagnosticArg::expected_byte_count(9));

    let abi = Diagnostic::new(
        DiagnosticId::new(1),
        DiagnosticKind::StandardLibraryRuntimeAbiMismatch,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::target_triple("test-target"))
    .with_arg(DiagnosticArg::expected_runtime_abi(
        DiagnosticRuntimeAbiVersion::new(2, 1),
    ))
    .with_arg(DiagnosticArg::actual_runtime_abi(
        DiagnosticRuntimeAbiVersion::new(1, 4),
    ));

    let renderer = DiagnosticRenderer::english();

    assert_eq!(
        renderer.render(&length).message(),
        "standard library artifact targets/test/1.0/libstd.a has 7 bytes but expected 9"
    );

    assert_eq!(
        renderer.render(&abi).message(),
        "target 'test-target' requires runtime ABI 2.1 but the standard library provides 1.4"
    );
}

#[test]
fn renderer_localizes_typed_runtime_metadata_failures_and_recovery() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::RuntimeArtifactMetadataInvalid,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::artifact_path("runtime/bray-runtime.brayrt"))
    .with_arg(DiagnosticArg::runtime_artifact_problem(
        DiagnosticRuntimeArtifactProblem::InvalidNativeIndexes,
    ))
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::RuntimeArtifactMustBeUsable,
    ));

    let rendered = DiagnosticRenderer::english().render(&diagnostic);

    assert_eq!(
        rendered.message(),
        "runtime artifact metadata is invalid: runtime/bray-runtime.brayrt: runtime native indexes do not cover each product category once"
    );

    let [note] = rendered.notes() else {
        panic!("expected one rendered note: {rendered:?}");
    };

    assert_eq!(note.rendered_kind(), RenderedDiagnosticNoteKind::Help);

    assert_eq!(
        note.message(),
        "select a readable runtime artifact built for the selected target and runtime ABI"
    );
}

#[test]
fn renderer_localizes_dependency_interface_context() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::InterfaceInvalidMagic,
        SeverityKind::Error,
    )
    .with_note(
        DiagnosticNote::new(DiagnosticNoteKind::InterfaceDependencyContext)
            .with_arg(DiagnosticArg::expected_package_identity(
                "example.dependency",
            ))
            .with_arg(DiagnosticArg::expected_product_identity("library"))
            .with_arg(DiagnosticArg::artifact_path("dependency.brayi")),
    );

    let rendered = DiagnosticRenderer::english().render(&diagnostic);

    let [note] = rendered.notes() else {
        panic!("expected one rendered note: {rendered:?}");
    };

    assert_eq!(note.rendered_kind(), RenderedDiagnosticNoteKind::Note);

    assert_eq!(
        note.message(),
        "while loading package 'example.dependency' product 'library' from dependency.brayi"
    );
}
