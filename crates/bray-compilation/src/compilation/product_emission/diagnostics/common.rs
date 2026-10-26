use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm,
    DiagnosticBag, DiagnosticEmissionFailure, DiagnosticId, DiagnosticKind, SeverityKind,
};
use bray_symbols::ProductIdentity;
use bray_target::TargetIdentity;

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
    Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::EmissionFailed,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::actual_product_identity(product.to_string()))
    .with_arg(DiagnosticArg::target_triple(target.as_str()))
    .with_arg(DiagnosticArg::emission_failure(failure.clone()))
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
        artifact.kind().diagnostic_kind(),
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

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticEmissionEvaluationFailure, DiagnosticEmissionFailure};
    use bray_symbols::{PackageIdentity, ProductIdentity};
    use bray_target::TargetIdentity;

    use super::emission_failure_diagnostic;

    #[test]
    fn capacity_failures_do_not_invent_source_locations_or_defect_guidance() {
        let product = ProductIdentity::try_new(
            PackageIdentity::try_new("example.package").expect("fixture package identity must be valid"),
            "application",
        )
        .expect("fixture product identity must be valid");

        let target = TargetIdentity::try_new("test-target").expect("fixture target identity must be valid");

        let diagnostic = emission_failure_diagnostic(
            DiagnosticEmissionFailure::Evaluation(
                DiagnosticEmissionEvaluationFailure::SemanticValueStoreCreate,
            ),
            &product,
            &target,
        );

        assert!(diagnostic.notes().is_empty());
        assert_eq!(diagnostic.primary_span(), None);
    }
}
