use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticEmissionFailure,
    DiagnosticEmissionLinkPlanFailure, DiagnosticEmissionPlanningFailure, DiagnosticId,
    DiagnosticKind, DiagnosticTestCatalogFailure, SeverityKind,
};
use bray_package_interface::{
    InterfaceValidationError, PackageImplementationArtifactBuildError,
    PackageInterfaceExportBuildError,
};
use bray_symbols::ProductIdentity;
use bray_target::TargetIdentity;

use super::common::emission_failure_diagnostics;
use super::evaluation::query_failure_diagnostics;
use super::linking::{link_plan_failure_diagnostics, staging_failure_diagnostics};
use super::model::ProductEmissionErrorKind;
use super::planning::planning_failure_diagnostics;
use super::terminal::{codegen_failure_diagnostics, package_interface_failure_diagnostics};

const fn package_interface_validation_error(
    kind: &ProductEmissionErrorKind,
) -> Option<&InterfaceValidationError> {
    match kind {
        ProductEmissionErrorKind::PackageInterfaceEncoding(error)
        | ProductEmissionErrorKind::PackageImplementation(
            PackageImplementationArtifactBuildError::InvalidBody(error)
            | PackageImplementationArtifactBuildError::InvalidArtifact(error),
        )
        | ProductEmissionErrorKind::PackageInterface(
            crate::compilation::PackageInterfaceExportError::Bundle(
                PackageInterfaceExportBuildError::Validation(error),
            ),
        ) => Some(error),
        _ => None,
    }
}

pub(super) fn product_emission_failure_diagnostics(
    kind: &ProductEmissionErrorKind,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> DiagnosticBag {
    match kind {
        ProductEmissionErrorKind::Cancelled => DiagnosticBag::new(),
        ProductEmissionErrorKind::ProductMismatch {
            requested,
            selected_package,
            ..
        } => DiagnosticBag::single(
            Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::EmissionProductMismatch,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::actual_product_identity(
                requested.to_string(),
            ))
            .with_arg(DiagnosticArg::expected_package_identity(
                selected_package.as_str(),
            )),
        ),
        ProductEmissionErrorKind::TargetMismatch {
            requested,
            selected,
        } => DiagnosticBag::single(
            Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::EmissionTargetMismatch,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::actual_product_identity(product.to_string()))
            .with_arg(DiagnosticArg::actual_target_triple(requested.as_str()))
            .with_arg(DiagnosticArg::expected_target_triple(selected.as_str())),
        ),
        ProductEmissionErrorKind::PackageInterfaceUnavailable
        | ProductEmissionErrorKind::PackageInterface(_)
        | ProductEmissionErrorKind::PackageInterfaceEncoding(_)
        | ProductEmissionErrorKind::PackageImplementation(_)
        | ProductEmissionErrorKind::PackageImplementationContent(_) => {
            match package_interface_validation_error(kind) {
                Some(error) => {
                    DiagnosticBag::single(error.clone().into_diagnostic(DiagnosticId::new(0)))
                }
                None => package_interface_failure_diagnostics(kind, product, target)
                    .unwrap_or_else(DiagnosticBag::new),
            }
        }
        ProductEmissionErrorKind::NativeIndexSizeLimitExceeded => emission_failure_diagnostics(
            DiagnosticEmissionFailure::NativeIndexSizeLimitExceeded,
            product,
            target,
        ),
        ProductEmissionErrorKind::MissingNativeInspector => emission_failure_diagnostics(
            DiagnosticEmissionFailure::NativeInspection {
                tool: None,
                path: None,
                reason: bray_diagnostics::DiagnosticNativeInspectionFailure::MissingToolchain,
            },
            product,
            target,
        ),
        ProductEmissionErrorKind::NativeInspection(error) => {
            use bray_diagnostics::{DiagnosticIoErrorKind, DiagnosticNativeInspectionFailure};
            use super::super::native::NativeInspectionError;

            let (tool, path, reason) = match error {
                NativeInspectionError::Read { path, kind } => (
                    None, path.clone(),
                    DiagnosticNativeInspectionFailure::Read(DiagnosticIoErrorKind::from(*kind)),
                ),
                NativeInspectionError::Invoke { tool, path, kind } => (
                    Some(*tool), path.clone(),
                    DiagnosticNativeInspectionFailure::Invoke(DiagnosticIoErrorKind::from(*kind)),
                ),
                NativeInspectionError::Failed { tool, path, status } => (
                    Some(*tool), path.clone(), DiagnosticNativeInspectionFailure::Failed(*status),
                ),
                NativeInspectionError::Encoding { tool, path } => (
                    Some(*tool), path.clone(), DiagnosticNativeInspectionFailure::Encoding,
                ),
            };

            emission_failure_diagnostics(
                DiagnosticEmissionFailure::NativeInspection { tool, path: Some(path), reason },
                product,
                target,
            )
        }
        ProductEmissionErrorKind::Planning(error) => {
            planning_failure_diagnostics(error, product, target)
        }
        ProductEmissionErrorKind::MissingTestCatalogArtifact => emission_failure_diagnostics(
            DiagnosticEmissionFailure::Planning(
                DiagnosticEmissionPlanningFailure::MissingTestCatalogArtifact,
            ),
            product,
            target,
        ),
        ProductEmissionErrorKind::TestCatalogContent(_) => emission_failure_diagnostics(
            DiagnosticEmissionFailure::TestCatalog(DiagnosticTestCatalogFailure::ResourceLimit),
            product,
            target,
        ),
        ProductEmissionErrorKind::InvalidCompilation => emission_failure_diagnostics(
            // rust-style: allow(context-erasing-failure-conversion, reason = "the emission diagnostic bag retains the exact causes")
            DiagnosticEmissionFailure::IncompleteProduct,
            product,
            target,
        ),
        ProductEmissionErrorKind::Codegen(error) => {
            codegen_failure_diagnostics(error, product, target)
        }
        ProductEmissionErrorKind::MissingLinker => emission_failure_diagnostics(
            DiagnosticEmissionFailure::LinkPlan(DiagnosticEmissionLinkPlanFailure::MissingLinker),
            product,
            target,
        ),
        ProductEmissionErrorKind::UnexpectedLinker => emission_failure_diagnostics(
            DiagnosticEmissionFailure::LinkPlan(
                DiagnosticEmissionLinkPlanFailure::UnexpectedLinker,
            ),
            product,
            target,
        ),
        ProductEmissionErrorKind::Staging(error) => {
            staging_failure_diagnostics(error, product, target)
        }
        ProductEmissionErrorKind::LinkPlan(error) => {
            link_plan_failure_diagnostics(error, product, target)
        }
        ProductEmissionErrorKind::Query(error) => query_failure_diagnostics(error, product, target),
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
    use bray_target::TargetIdentity;
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::product_emission_failure_diagnostics;
    use crate::compilation::ProductEmissionErrorKind;

    #[test]
    fn request_routing_failures_preserve_both_selected_identities() {
        let requested_package = PackageIdentity::try_new("requested.package")
            .unwrap_or_else(|| panic!("requested package identity must be valid"));

        let requested = ProductIdentity::try_new(requested_package, "application")
            .unwrap_or_else(|| panic!("requested product identity must be valid"));

        let selected_package = PackageIdentity::try_new("selected.package")
            .unwrap_or_else(|| panic!("selected package identity must be valid"));

        let selected_target = TargetIdentity::try_new("selected-target")
            .unwrap_or_else(|| panic!("selected target identity must be valid"));

        let product_mismatch = product_emission_failure_diagnostics(
            &ProductEmissionErrorKind::ProductMismatch {
                requested: requested.clone(),
                selected_package,
                selected_kind: ProductKind::Executable,
            },
            &requested,
            &selected_target,
        );

        assert_goal_state_diagnostic_kind(
            &product_mismatch,
            DiagnosticKind::EmissionProductMismatch,
        );

        let requested_target = TargetIdentity::try_new("requested-target")
            .unwrap_or_else(|| panic!("requested target identity must be valid"));

        let target_mismatch = product_emission_failure_diagnostics(
            &ProductEmissionErrorKind::TargetMismatch {
                requested: requested_target,
                selected: selected_target.clone(),
            },
            &requested,
            &selected_target,
        );

        assert_goal_state_diagnostic_kind(&target_mismatch, DiagnosticKind::EmissionTargetMismatch);
    }
}
