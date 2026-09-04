use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind,
    DiagnosticProjectSelectionProblem, DiagnosticReusableBuildIdentityPart, SeverityKind,
};
use bray_emitter::ProductBuildIdentityPart;

const fn diagnostic_part(part: ProductBuildIdentityPart) -> DiagnosticReusableBuildIdentityPart {
    match part {
        ProductBuildIdentityPart::Inputs => DiagnosticReusableBuildIdentityPart::Inputs,
        ProductBuildIdentityPart::Compiler => DiagnosticReusableBuildIdentityPart::Compiler,
        ProductBuildIdentityPart::Toolchain => DiagnosticReusableBuildIdentityPart::Toolchain,
        ProductBuildIdentityPart::StandardLibrary => {
            DiagnosticReusableBuildIdentityPart::StandardLibrary
        }
        ProductBuildIdentityPart::Runtime => DiagnosticReusableBuildIdentityPart::Runtime,
        ProductBuildIdentityPart::CatalogProtocol => {
            DiagnosticReusableBuildIdentityPart::CatalogProtocol
        }
        ProductBuildIdentityPart::RunnerProtocol => {
            DiagnosticReusableBuildIdentityPart::RunnerProtocol
        }
    }
}

/// Returns the shared structured diagnostic for one reusable build-identity mismatch.
pub fn reusable_build_identity_mismatch_diagnostics(
    product: String,
    part: ProductBuildIdentityPart,
) -> DiagnosticBag {
    DiagnosticBag::single(
        Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::ProjectCommandSelectionInvalid,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::project_selection_problem(
            DiagnosticProjectSelectionProblem::ReusableBuildIdentityMismatch {
                product,
                part: diagnostic_part(part),
            },
        )),
    )
}
