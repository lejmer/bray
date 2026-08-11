use std::path::PathBuf;

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticNote, DiagnosticNoteKind,
    SeverityKind,
};
use bray_symbols::ProductIdentity;

use super::model::{ProjectManifestProblem, ProjectManifestViolation};

pub(super) fn invalid_manifest_diagnostic(
    id: DiagnosticId,
    path: PathBuf,
    problem: ProjectManifestProblem,
) -> Diagnostic {
    let base = |kind, field| {
        Diagnostic::new(id, kind, SeverityKind::Error)
            .with_arg(DiagnosticArg::file_path(path.clone()))
            .with_arg(DiagnosticArg::project_manifest_field(field))
    };

    match problem.into_violation() {
        ProjectManifestViolation::UnsupportedFormat {
            field,
            actual,
            supported,
        } => base(DiagnosticKind::ProjectManifestUnsupportedFormat, field)
            .with_arg(DiagnosticArg::actual_revision(actual))
            .with_arg(DiagnosticArg::expected_revision(supported)),
        ProjectManifestViolation::InvalidPath { field, path } => {
            base(DiagnosticKind::ProjectManifestInvalidPath, field)
                .with_arg(DiagnosticArg::project_path(path))
        }
        ProjectManifestViolation::InvalidName { field, value } => {
            base(DiagnosticKind::ProjectManifestInvalidName, field)
                .with_arg(DiagnosticArg::referenced_name(value))
        }
        ProjectManifestViolation::InvalidPackageVersion { field, value } => {
            base(DiagnosticKind::ProjectPackageVersionInvalid, field)
                .with_arg(DiagnosticArg::referenced_name(value))
                .with_note(DiagnosticNote::new(
                    DiagnosticNoteKind::PackageVersionMustBeValid,
                ))
        }
        ProjectManifestViolation::MissingWorkspacePackageVersion { field } => {
            base(DiagnosticKind::ProjectPackageVersionMissingWorkspace, field).with_note(
                DiagnosticNote::new(DiagnosticNoteKind::PackageVersionMustBeValid),
            )
        }
        ProjectManifestViolation::ReservedPackageIdentity { field, identity } => {
            base(DiagnosticKind::ProjectPackageIdentityReserved, field)
                .with_arg(DiagnosticArg::actual_package_identity(identity))
        }
        ProjectManifestViolation::StandardLibraryPackageIdentityRequired { field, identity } => {
            base(
                DiagnosticKind::ProjectStandardLibraryPackageIdentityRequired,
                field,
            )
            .with_arg(DiagnosticArg::actual_package_identity(identity))
        }
        ProjectManifestViolation::StandardLibraryRootPackageRequired { field, identity } => base(
            DiagnosticKind::ProjectStandardLibraryRootPackageRequired,
            field,
        )
        .with_arg(DiagnosticArg::actual_package_identity(identity)),
        ProjectManifestViolation::MissingSelection { field } => {
            base(DiagnosticKind::ProjectManifestMissingSelection, field)
        }
        ProjectManifestViolation::DuplicateSelection { field, value } => {
            base(DiagnosticKind::ProjectManifestDuplicateSelection, field)
                .with_arg(DiagnosticArg::referenced_name(value))
        }
        ProjectManifestViolation::MissingRootPackage { field } => {
            base(DiagnosticKind::ProjectManifestMissingRootPackage, field)
        }
        ProjectManifestViolation::UndeclaredFeature { field, feature } => {
            base(DiagnosticKind::ProjectManifestUndeclaredFeature, field)
                .with_arg(DiagnosticArg::referenced_name(feature))
        }
        ProjectManifestViolation::UnknownSourceRoot { field, root } => {
            base(DiagnosticKind::ProjectManifestUnknownSourceRoot, field)
                .with_arg(DiagnosticArg::referenced_name(root))
        }
        ProjectManifestViolation::UnknownTarget { field, target } => {
            base(DiagnosticKind::ProjectManifestUnknownTarget, field)
                .with_arg(DiagnosticArg::target_triple(target))
        }
        ProjectManifestViolation::UnknownTargetPredicateProperty { field, property } => base(
            DiagnosticKind::ProjectManifestUnknownTargetPredicateProperty,
            field,
        )
        .with_arg(DiagnosticArg::referenced_name(property)),
        ProjectManifestViolation::TargetPredicateValueKindMismatch {
            field,
            property,
            expected,
            actual,
        } => base(
            DiagnosticKind::ProjectManifestTargetPredicateValueKindMismatch,
            field,
        )
        .with_arg(DiagnosticArg::referenced_name(property.as_str()))
        .with_arg(DiagnosticArg::expected_target_predicate_value_kind(
            expected.into(),
        ))
        .with_arg(DiagnosticArg::actual_target_predicate_value_kind(
            actual.into(),
        )),
        ProjectManifestViolation::UnknownDependencyPackage { field, package } => {
            base(DiagnosticKind::ProjectDependencyPackageUnknown, field)
                .with_arg(DiagnosticArg::actual_package_identity(package.as_str()))
        }
        ProjectManifestViolation::UnknownDependencyProduct { field, product } => {
            product_diagnostic(
                base(DiagnosticKind::ProjectDependencyProductUnknown, field),
                product,
            )
        }
        ProjectManifestViolation::DependencyProductNotLibrary { field, product } => {
            product_diagnostic(
                base(DiagnosticKind::ProjectDependencyProductNotLibrary, field),
                product,
            )
        }
        ProjectManifestViolation::DependencyTargetUnavailable {
            field,
            product,
            target,
        } => product_diagnostic(
            base(
                DiagnosticKind::ProjectDependencyProductTargetUnavailable,
                field,
            ),
            product,
        )
        .with_arg(DiagnosticArg::target_triple(target.as_str())),
        ProjectManifestViolation::TestedLibraryOnNonTestProduct { field, product } => base(
            DiagnosticKind::ProjectManifestUnexpectedTestedLibrary,
            field,
        )
        .with_arg(DiagnosticArg::actual_product_identity(product)),
        ProjectManifestViolation::InvalidSourceRoot { field, path } => {
            base(DiagnosticKind::ProjectSourceRootInvalid, field)
                .with_arg(DiagnosticArg::project_path(path))
        }
        ProjectManifestViolation::SourceSymlink { field, path } => {
            base(DiagnosticKind::ProjectSourceRootContainsSymlink, field)
                .with_arg(DiagnosticArg::project_path(path))
        }
        ProjectManifestViolation::NonUtf8SourcePath { field, path } => {
            base(DiagnosticKind::ProjectSourceRootContainsNonUtf8Path, field)
                .with_arg(DiagnosticArg::project_path(path))
        }
        ProjectManifestViolation::DependencyCyclePackage { field, package } => {
            base(DiagnosticKind::ProjectDependencyCycle, field).with_arg(
                DiagnosticArg::project_dependency_cycle_member(
                    bray_diagnostics::DiagnosticProjectDependencyCycleMember::Package {
                        identity: package.as_str().to_owned(),
                    },
                ),
            )
        }
        ProjectManifestViolation::DependencyCycleProduct { field, product } => {
            base(DiagnosticKind::ProjectDependencyCycle, field).with_arg(
                DiagnosticArg::project_dependency_cycle_member(
                    bray_diagnostics::DiagnosticProjectDependencyCycleMember::Product {
                        package: product.package().as_str().to_owned(),
                        product: product.name().to_owned(),
                    },
                ),
            )
        }
    }
}

fn product_diagnostic(diagnostic: Diagnostic, product: ProductIdentity) -> Diagnostic {
    diagnostic
        .with_arg(DiagnosticArg::actual_package_identity(
            product.package().as_str(),
        ))
        .with_arg(DiagnosticArg::actual_product_identity(product.name()))
}
