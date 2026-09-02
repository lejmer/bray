use bray_codegen::BackendArtifactKind;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm,
    DiagnosticArtifactKind, DiagnosticBag, DiagnosticEmissionFailure, DiagnosticId, DiagnosticKind,
    SeverityKind,
};
use bray_symbols::ProductIdentity;
use bray_target::TargetIdentity;

pub(super) const fn diagnostic_backend_artifact_kind(
    kind: BackendArtifactKind,
) -> DiagnosticArtifactKind {
    match kind {
        BackendArtifactKind::RelocatableObject => DiagnosticArtifactKind::RelocatableObject,
        BackendArtifactKind::Assembly => DiagnosticArtifactKind::Assembly,
        BackendArtifactKind::BackendIr => DiagnosticArtifactKind::BackendIr,
        BackendArtifactKind::BackendBitcode => DiagnosticArtifactKind::BackendBitcode,
        BackendArtifactKind::ExecutableModule => DiagnosticArtifactKind::ExecutableModule,
        BackendArtifactKind::DebugCompanion => DiagnosticArtifactKind::DebugCompanion,
    }
}

pub(super) fn emission_failure_diagnostics(
    failure: DiagnosticEmissionFailure,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> DiagnosticBag {
    DiagnosticBag::single(emission_failure_diagnostic(failure, product, target))
}

pub(super) fn emission_failure_diagnostic(
    failure: DiagnosticEmissionFailure,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::EmissionFailed,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::actual_product_identity(product.to_string()))
    .with_arg(DiagnosticArg::target_triple(target.as_str()))
    .with_arg(DiagnosticArg::emission_failure(failure.clone()));

    let source = match &failure {
        DiagnosticEmissionFailure::Evaluation(failure) => {
            crate::compilation::diagnostics::code_production_failure_source(failure)
        }
        _ => None,
    };

    match source {
        Some(source) => {
            crate::compilation::diagnostics::with_compiler_defect_source(diagnostic, source)
        }
        None if matches!(&failure, DiagnosticEmissionFailure::Evaluation(_)) => {
            crate::compilation::diagnostics::with_compiler_defect_note(diagnostic)
        }
        None => diagnostic,
    }
}

pub(super) const fn diagnostic_artifact(
    artifact: &bray_emitter::ArtifactId,
) -> bray_diagnostics::DiagnosticEmissionArtifact {
    bray_diagnostics::DiagnosticEmissionArtifact::new(
        artifact.kind().diagnostic_kind(),
        artifact.ordinal(),
    )
}

pub(super) const fn diagnostic_backend_artifact(
    artifact: &bray_codegen::BackendArtifactId,
) -> bray_diagnostics::DiagnosticEmissionArtifact {
    bray_diagnostics::DiagnosticEmissionArtifact::new(
        diagnostic_backend_artifact_kind(artifact.kind()),
        artifact.ordinal(),
    )
}

pub(super) fn codegen_unit_identity(
    unit: &bray_codegen::CodegenUnitKey,
) -> DiagnosticArtifactDigest {
    DiagnosticArtifactDigest::new(
        DiagnosticArtifactDigestAlgorithm::Blake3,
        unit.content_identity(),
    )
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticCheckerFailure, DiagnosticEmissionEvaluationFailure, DiagnosticEmissionFailure,
        DiagnosticNoteKind,
    };
    use bray_symbols::{PackageIdentity, ProductIdentity};
    use bray_target::TargetIdentity;

    use super::emission_failure_diagnostic;

    #[test]
    fn evaluation_failures_without_source_locations_explain_how_to_report_the_defect() {
        let package = PackageIdentity::try_new("example.package")
            .unwrap_or_else(|| panic!("package identity must be valid"));

        let product = ProductIdentity::try_new(package, "application")
            .unwrap_or_else(|| panic!("product identity must be valid"));

        let target = TargetIdentity::try_new("test-target")
            .unwrap_or_else(|| panic!("target identity must be valid"));

        let failure =
            DiagnosticEmissionFailure::Evaluation(DiagnosticEmissionEvaluationFailure::Checker(
                DiagnosticCheckerFailure::CompilerKnownRepresentationUnavailable("ScalarU32"),
            ));

        let diagnostic = emission_failure_diagnostic(failure, &product, &target);

        assert!(
            diagnostic
                .notes()
                .iter()
                .any(|note| note.kind() == DiagnosticNoteKind::ReportCompilerDefect)
        );
    }
}

pub(super) fn emission_artifact_diagnostic(
    kind: DiagnosticKind,
    artifact: &bray_emitter::ArtifactId,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error)
        .with_arg(DiagnosticArg::actual_product_identity(product.to_string()))
        .with_arg(DiagnosticArg::target_triple(target.as_str()))
        .with_arg(DiagnosticArg::artifact_kind(
            artifact.kind().diagnostic_kind(),
        ))
        .with_arg(DiagnosticArg::artifact_ordinal(artifact.ordinal()))
}
