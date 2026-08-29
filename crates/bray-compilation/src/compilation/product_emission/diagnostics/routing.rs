use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm,
    DiagnosticBag, DiagnosticEmissionFailure, DiagnosticEmissionLinkPlanFailure,
    DiagnosticEmissionPlanningFailure, DiagnosticId, DiagnosticKind, SeverityKind,
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
    if let ProductEmissionErrorKind::ProductMismatch {
        requested,
        selected_package,
        ..
    } = kind
    {
        return DiagnosticBag::single(
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
        );
    }

    if let ProductEmissionErrorKind::TargetMismatch {
        requested,
        selected,
    } = kind
    {
        return DiagnosticBag::single(
            Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::EmissionTargetMismatch,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::actual_product_identity(product.to_string()))
            .with_arg(DiagnosticArg::actual_target_triple(requested.as_str()))
            .with_arg(DiagnosticArg::expected_target_triple(selected.as_str())),
        );
    }

    if let ProductEmissionErrorKind::Staging(error) = kind {
        return staging_failure_diagnostics(error, product, target);
    }

    if let ProductEmissionErrorKind::Planning(error) = kind {
        return planning_failure_diagnostics(error, product, target);
    }

    if let ProductEmissionErrorKind::LinkPlan(error) = kind {
        return link_plan_failure_diagnostics(error, product, target);
    }

    if let Some(error) = package_interface_validation_error(kind) {
        return DiagnosticBag::single(error.clone().into_diagnostic(DiagnosticId::new(0)));
    }

    if let Some(diagnostics) = package_interface_failure_diagnostics(kind, product, target) {
        return diagnostics;
    }

    if let ProductEmissionErrorKind::Query(error) = kind {
        return query_failure_diagnostics(error, product, target);
    }

    if let ProductEmissionErrorKind::Codegen(error) = kind {
        return codegen_failure_diagnostics(error, product, target);
    }

    if let ProductEmissionErrorKind::Outcome(error) = kind {
        let outer = emission_failure_diagnostics(
            DiagnosticEmissionFailure::IncompleteProduct,
            product,
            target,
        );

        return error.diagnostics().merged(&outer);
    }

    let failure = match kind {
        ProductEmissionErrorKind::Cancelled => return DiagnosticBag::new(),
        ProductEmissionErrorKind::ProductMismatch { .. }
        | ProductEmissionErrorKind::TargetMismatch { .. } => {
            DiagnosticEmissionFailure::InvalidRequest
        }
        ProductEmissionErrorKind::PackageInterfaceUnavailable
        | ProductEmissionErrorKind::PackageInterface(_)
        | ProductEmissionErrorKind::PackageInterfaceEncoding(_)
        | ProductEmissionErrorKind::PackageImplementation(_)
        | ProductEmissionErrorKind::PackageImplementationContent(_) => {
            return package_interface_failure_diagnostics(kind, product, target)
                .unwrap_or_else(DiagnosticBag::new);
        }
        ProductEmissionErrorKind::Planning(_)
        | ProductEmissionErrorKind::MissingExecutableHost
        | ProductEmissionErrorKind::MissingRootFrame(_) => match kind {
            ProductEmissionErrorKind::MissingExecutableHost => DiagnosticEmissionFailure::Planning(
                DiagnosticEmissionPlanningFailure::MissingExecutableHost,
            ),
            ProductEmissionErrorKind::MissingRootFrame(frame) => {
                DiagnosticEmissionFailure::Planning(
                    DiagnosticEmissionPlanningFailure::MissingRootFrame(
                        DiagnosticArtifactDigest::new(
                            DiagnosticArtifactDigestAlgorithm::Blake3,
                            frame.digest(),
                        ),
                    ),
                )
            }
            _ => DiagnosticEmissionFailure::Planning(DiagnosticEmissionPlanningFailure::Incomplete),
        },
        ProductEmissionErrorKind::InvalidCompilation => {
            DiagnosticEmissionFailure::IncompleteProduct
        }
        ProductEmissionErrorKind::Codegen(_) => DiagnosticEmissionFailure::Codegen(
            bray_diagnostics::DiagnosticEmissionCodegenFailure::Incomplete,
        ),
        ProductEmissionErrorKind::MissingLinker => {
            DiagnosticEmissionFailure::LinkPlan(DiagnosticEmissionLinkPlanFailure::MissingLinker)
        }
        ProductEmissionErrorKind::UnexpectedLinker => {
            DiagnosticEmissionFailure::LinkPlan(DiagnosticEmissionLinkPlanFailure::UnexpectedLinker)
        }
        ProductEmissionErrorKind::Staging(_) => DiagnosticEmissionFailure::Staging(
            bray_diagnostics::DiagnosticEmissionStagingFailure::Incomplete,
        ),
        ProductEmissionErrorKind::LinkPlan(_) => {
            DiagnosticEmissionFailure::LinkPlan(DiagnosticEmissionLinkPlanFailure::Incomplete)
        }
        ProductEmissionErrorKind::Query(_) => DiagnosticEmissionFailure::Evaluation(
            bray_diagnostics::DiagnosticEmissionEvaluationFailure::Infrastructure,
        ),
        ProductEmissionErrorKind::Outcome(_) => DiagnosticEmissionFailure::IncompleteProduct,
    };

    DiagnosticBag::single(
        Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::EmissionFailed,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::actual_product_identity(product.to_string()))
        .with_arg(DiagnosticArg::target_triple(target.as_str()))
        .with_arg(DiagnosticArg::emission_failure(failure)),
    )
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
