use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm,
    DiagnosticBag, DiagnosticEmissionCodegenFailure, DiagnosticEmissionEvaluationFailure,
    DiagnosticEmissionFailure, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind,
    DiagnosticPackageInterfaceFailure, SeverityKind,
};
use bray_emitter::BackendContributionMergeErrorKind;
use bray_package_interface::{
    InterfaceValidationError, PackageImplementationArtifactBuildError,
    PackageInterfaceExportBuildError, PackageInterfaceExportSurfaceError,
    PackageInterfaceSurfaceBuildError,
};
use bray_symbols::{ImportedIdentitySurfaceError, ProductIdentity};
use bray_target::TargetIdentity;

use super::common::{
    diagnostic_backend_artifact, emission_failure_diagnostic, emission_failure_diagnostics,
};
use super::model::ProductEmissionErrorKind;
use crate::compilation::product::codegen_fact_failure_kind;
use crate::compilation::{
    CodegenFactError, EmissionCodegenError, EmissionCodegenErrorKind, PackageInterfaceExportError,
};
use crate::fact::FactQueryError;

pub(super) fn codegen_failure_diagnostics(
    error: &EmissionCodegenError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> DiagnosticBag {
    let failure = match error.kind() {
        EmissionCodegenErrorKind::DuplicateUnit(unit) => {
            DiagnosticEmissionCodegenFailure::DuplicateUnit(codegen_unit_identity(unit))
        }
        EmissionCodegenErrorKind::DuplicateMappings(unit) => {
            DiagnosticEmissionCodegenFailure::DuplicateMappings(codegen_unit_identity(unit))
        }
        EmissionCodegenErrorKind::MissingUnit(unit) => {
            DiagnosticEmissionCodegenFailure::MissingUnit(codegen_unit_identity(unit))
        }
        EmissionCodegenErrorKind::MissingMappings(unit) => {
            DiagnosticEmissionCodegenFailure::MissingMappings(codegen_unit_identity(unit))
        }
        EmissionCodegenErrorKind::Request { unit: _, error } => {
            match error.as_ref() {
                CodegenFactError::Diagnostics(diagnostics) => return diagnostics.clone(),
                CodegenFactError::Query(error) => {
                    return query_failure_diagnostics(error, product, target);
                }
                _ => {}
            }

            let failure = codegen_fact_failure_kind(error)
                .unwrap_or_else(|| unreachable!("query and diagnostic failures return above"));

            return DiagnosticBag::single(
                Diagnostic::new(
                    DiagnosticId::new(0),
                    DiagnosticKind::NativeProductPreparationFailed,
                    SeverityKind::Error,
                )
                .with_arg(DiagnosticArg::actual_product_identity(product.to_string()))
                .with_arg(DiagnosticArg::target_triple(target.as_str()))
                .with_arg(DiagnosticArg::native_product_failure_kind(failure)),
            );
        }
        EmissionCodegenErrorKind::Generation { unit, .. } => {
            DiagnosticEmissionCodegenFailure::Generation(codegen_unit_identity(unit))
        }
        EmissionCodegenErrorKind::Cancelled(_) => return DiagnosticBag::new(),
        EmissionCodegenErrorKind::Query(error) => {
            return query_failure_diagnostics(error, product, target);
        }
        EmissionCodegenErrorKind::InvalidContributions(error) => {
            return contribution_merge_failure_diagnostics(error.kind(), product, target);
        }
    };

    emission_failure_diagnostics(DiagnosticEmissionFailure::Codegen(failure), product, target)
}

fn contribution_merge_failure_diagnostics(
    error: &BackendContributionMergeErrorKind,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> DiagnosticBag {
    let failure = match error {
        BackendContributionMergeErrorKind::Cancelled => return DiagnosticBag::new(),
        BackendContributionMergeErrorKind::DuplicateUnit(unit) => {
            DiagnosticEmissionCodegenFailure::MergeDuplicateUnit(codegen_unit_identity(unit))
        }
        BackendContributionMergeErrorKind::MissingUnit(unit) => {
            DiagnosticEmissionCodegenFailure::MergeMissingUnit(codegen_unit_identity(unit))
        }
        BackendContributionMergeErrorKind::UnrequestedUnit(unit) => {
            DiagnosticEmissionCodegenFailure::MergeUnrequestedUnit(codegen_unit_identity(unit))
        }
        BackendContributionMergeErrorKind::BackendMismatch(unit) => {
            DiagnosticEmissionCodegenFailure::MergeBackendMismatch(codegen_unit_identity(unit))
        }
        BackendContributionMergeErrorKind::CapabilityRevisionMismatch(unit) => {
            DiagnosticEmissionCodegenFailure::MergeCapabilityMismatch(codegen_unit_identity(unit))
        }
        BackendContributionMergeErrorKind::TargetMismatch(unit) => {
            DiagnosticEmissionCodegenFailure::MergeTargetMismatch(codegen_unit_identity(unit))
        }
        BackendContributionMergeErrorKind::MissingArtifact(artifact) => {
            DiagnosticEmissionCodegenFailure::MergeMissingArtifact(diagnostic_backend_artifact(
                artifact,
            ))
        }
        BackendContributionMergeErrorKind::UnrequestedArtifact(artifact) => {
            DiagnosticEmissionCodegenFailure::MergeUnrequestedArtifact(diagnostic_backend_artifact(
                artifact,
            ))
        }
        BackendContributionMergeErrorKind::ArtifactKindMismatch(artifact) => {
            DiagnosticEmissionCodegenFailure::MergeArtifactKindMismatch(
                diagnostic_backend_artifact(artifact),
            )
        }
        BackendContributionMergeErrorKind::Read { artifact, kind } => {
            DiagnosticEmissionCodegenFailure::MergeReadFailed {
                artifact: diagnostic_backend_artifact(artifact),
                error: DiagnosticIoErrorKind::from(*kind),
            }
        }
        BackendContributionMergeErrorKind::LengthMismatch {
            artifact,
            expected,
            actual,
        } => DiagnosticEmissionCodegenFailure::MergeLengthMismatch {
            artifact: diagnostic_backend_artifact(artifact),
            expected: *expected,
            actual: *actual,
        },
        BackendContributionMergeErrorKind::DigestMismatch {
            artifact,
            expected,
            actual,
        } => DiagnosticEmissionCodegenFailure::MergeDigestMismatch {
            artifact: diagnostic_backend_artifact(artifact),
            expected: expected.diagnostic_digest(),
            actual: actual.diagnostic_digest(),
        },
        BackendContributionMergeErrorKind::InvalidContent(artifact) => {
            DiagnosticEmissionCodegenFailure::MergeInvalidContent(diagnostic_backend_artifact(
                artifact,
            ))
        }
    };

    emission_failure_diagnostics(DiagnosticEmissionFailure::Codegen(failure), product, target)
}

fn codegen_unit_identity(unit: &bray_codegen::CodegenUnitKey) -> DiagnosticArtifactDigest {
    DiagnosticArtifactDigest::new(
        DiagnosticArtifactDigestAlgorithm::Blake3,
        unit.content_identity(),
    )
}

pub(super) const fn package_interface_validation_error(
    kind: &ProductEmissionErrorKind,
) -> Option<&InterfaceValidationError> {
    match kind {
        ProductEmissionErrorKind::PackageInterfaceEncoding(error)
        | ProductEmissionErrorKind::PackageImplementation(
            PackageImplementationArtifactBuildError::InvalidBody(error)
            | PackageImplementationArtifactBuildError::InvalidArtifact(error),
        )
        | ProductEmissionErrorKind::PackageInterface(PackageInterfaceExportError::Bundle(
            PackageInterfaceExportBuildError::Validation(error),
        )) => Some(error),
        _ => None,
    }
}

pub(super) fn package_interface_failure_diagnostics(
    kind: &ProductEmissionErrorKind,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Option<DiagnosticBag> {
    let diagnostic = match kind {
        ProductEmissionErrorKind::PackageInterfaceUnavailable => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::Unavailable,
            product,
            target,
        ),
        ProductEmissionErrorKind::PackageInterface(error) => {
            package_interface_export_failure_diagnostic(error, product, target)?
        }
        ProductEmissionErrorKind::PackageImplementation(error) => {
            package_implementation_failure_diagnostic(error, product, target)?
        }
        ProductEmissionErrorKind::PackageImplementationContent(_) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::ImplementationContentTooLarge,
            product,
            target,
        ),
        ProductEmissionErrorKind::PackageInterfaceEncoding(_) => return None,
        _ => return None,
    };

    Some(DiagnosticBag::single(diagnostic))
}

fn package_failure_diagnostic(
    failure: DiagnosticPackageInterfaceFailure,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    emission_failure_diagnostic(
        DiagnosticEmissionFailure::PackageInterface(failure),
        product,
        target,
    )
}

fn package_interface_export_failure_diagnostic(
    error: &PackageInterfaceExportError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Option<Diagnostic> {
    let diagnostic = match error {
        PackageInterfaceExportError::InvalidCompilation => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::InvalidCompilation,
            product,
            target,
        ),
        PackageInterfaceExportError::RecoveredPublicSymbol(kind) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::RecoveredPublicSymbol(kind.as_str().to_owned()),
            product,
            target,
        ),
        PackageInterfaceExportError::IncompletePublicDeclarationFacts(kind) => {
            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::IncompletePublicDeclaration(
                    kind.as_str().to_owned(),
                ),
                product,
                target,
            )
        }
        PackageInterfaceExportError::Surface(error) => {
            package_interface_surface_failure_diagnostic(error, product, target)
        }
        PackageInterfaceExportError::Bundle(error) => {
            package_interface_bundle_failure_diagnostic(error, product, target)?
        }
    };

    Some(diagnostic)
}

fn package_interface_surface_failure_diagnostic(
    error: &PackageInterfaceExportSurfaceError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    match error {
        PackageInterfaceExportSurfaceError::DuplicateSymbol(key) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::DuplicateSymbol(key.kind().as_str().to_owned()),
            product,
            target,
        ),
        PackageInterfaceExportSurfaceError::MissingSymbol(key) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::MissingSymbol(key.kind().as_str().to_owned()),
            product,
            target,
        ),
        PackageInterfaceExportSurfaceError::SymbolCountOverflow => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::SymbolCountOverflow,
            product,
            target,
        ),
        PackageInterfaceExportSurfaceError::Surface(error) => {
            package_interface_structural_failure_diagnostic(error, product, target)
        }
    }
}

fn package_interface_bundle_failure_diagnostic(
    error: &PackageInterfaceExportBuildError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Option<Diagnostic> {
    let failure = match error {
        PackageInterfaceExportBuildError::MissingSemanticFacts(key) => {
            return Some(package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::MissingSemanticContent(
                    key.kind().as_str().to_owned(),
                ),
                product,
                target,
            ));
        }
        PackageInterfaceExportBuildError::Validation(_) => return None,
        PackageInterfaceExportBuildError::DuplicateExecutableTemplate(owner) => {
            DiagnosticPackageInterfaceFailure::DuplicateExecutableTemplate(owner.raw())
        }
        PackageInterfaceExportBuildError::InvalidExecutableTemplateFamily(owner) => {
            DiagnosticPackageInterfaceFailure::InvalidExecutableTemplateFamily(owner.raw())
        }
        PackageInterfaceExportBuildError::DuplicateNativeBoundary(owner) => {
            DiagnosticPackageInterfaceFailure::DuplicateNativeBoundary(owner.raw())
        }
    };

    Some(package_failure_diagnostic(failure, product, target))
}

fn package_implementation_failure_diagnostic(
    error: &PackageImplementationArtifactBuildError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Option<Diagnostic> {
    let failure = match error {
        PackageImplementationArtifactBuildError::DuplicateCallableBody(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationDuplicateCallableBody(owner.raw())
        }
        PackageImplementationArtifactBuildError::DuplicateExecutableTemplate(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationDuplicateExecutableTemplate(
                owner.raw(),
            )
        }
        PackageImplementationArtifactBuildError::InvalidExecutableTemplateFamily(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationInvalidExecutableTemplateFamily(
                owner.raw(),
            )
        }
        PackageImplementationArtifactBuildError::DuplicateNativeBoundary(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationDuplicateNativeBoundary(owner.raw())
        }
        PackageImplementationArtifactBuildError::DuplicateSpecialization => {
            DiagnosticPackageInterfaceFailure::ImplementationDuplicateSpecialization
        }
        PackageImplementationArtifactBuildError::SpecializationIdentityMismatch => {
            DiagnosticPackageInterfaceFailure::ImplementationSpecializationIdentityMismatch
        }
        PackageImplementationArtifactBuildError::InvalidExecutableOwner(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationInvalidExecutableOwner(owner.raw())
        }
        PackageImplementationArtifactBuildError::InvalidNativeBoundaryOwner(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationInvalidNativeBoundaryOwner(owner.raw())
        }
        PackageImplementationArtifactBuildError::InvalidCallableOwner(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationInvalidCallableOwner(owner.raw())
        }
        PackageImplementationArtifactBuildError::InvalidBody(_)
        | PackageImplementationArtifactBuildError::InvalidArtifact(_) => return None,
    };

    Some(package_failure_diagnostic(failure, product, target))
}

fn package_interface_structural_failure_diagnostic(
    error: &PackageInterfaceSurfaceBuildError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    match error {
        PackageInterfaceSurfaceBuildError::NonLibraryProduct => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::NonLibraryProduct,
            product,
            target,
        ),
        PackageInterfaceSurfaceBuildError::DependencyCountOverflow => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::DependencyCountOverflow,
            product,
            target,
        ),
        PackageInterfaceSurfaceBuildError::DuplicateDependencyPackage(package) => {
            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::DuplicateDependencyPackage(
                    package.as_str().to_owned(),
                ),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::NonCanonicalSymbolOrder { previous, current } => {
            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::NonCanonicalSymbolOrder {
                    previous: previous.raw(),
                    current: current.raw(),
                },
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::Identity(error) => {
            package_interface_identity_failure_diagnostic(error, product, target)
        }
        PackageInterfaceSurfaceBuildError::RelationshipSymbolOutOfBounds(relationship) => {
            package_failure_diagnostic(
                relationship_failure(*relationship, RelationshipFailureKind::SymbolOutOfBounds),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::InvalidRelationship(relationship) => {
            package_failure_diagnostic(
                relationship_failure(*relationship, RelationshipFailureKind::Invalid),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::DuplicateRelationshipPosition(relationship) => {
            package_failure_diagnostic(
                relationship_failure(*relationship, RelationshipFailureKind::DuplicatePosition),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::ExportOwnerOutOfBounds(owner) => {
            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::ExportOwnerOutOfBounds(owner.raw()),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::InvalidExportOwner(owner) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::InvalidExportOwner(owner.raw()),
            product,
            target,
        ),
        PackageInterfaceSurfaceBuildError::ExportTargetOutOfBounds(target_symbol) => {
            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::ExportTargetOutOfBounds(target_symbol.raw()),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::DependencyOutOfBounds(dependency) => {
            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::DependencyOutOfBounds(dependency.raw()),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::DependencyKeyPackageMismatch(dependency) => {
            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::DependencyKeyPackageMismatch(dependency.raw()),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::InvalidDirectExportTarget(target_symbol) => {
            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::InvalidDirectExportTarget(target_symbol.raw()),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::DuplicateExportName { owner, name } => {
            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::DuplicateExportName {
                    owner: owner.raw(),
                    name: name.as_str().to_owned(),
                },
                product,
                target,
            )
        }
    }
}

fn package_interface_identity_failure_diagnostic(
    error: &ImportedIdentitySurfaceError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    let failure = match error {
        ImportedIdentitySurfaceError::Empty => DiagnosticPackageInterfaceFailure::IdentityEmpty,
        ImportedIdentitySurfaceError::SymbolCountOverflow => {
            DiagnosticPackageInterfaceFailure::IdentitySymbolCountOverflow
        }
        ImportedIdentitySurfaceError::NonCanonicalSymbolId { expected, actual } => {
            DiagnosticPackageInterfaceFailure::IdentityNonCanonicalSymbolId {
                expected: expected.raw(),
                actual: actual.raw(),
            }
        }
        ImportedIdentitySurfaceError::MissingPackageRoot { actual } => {
            DiagnosticPackageInterfaceFailure::IdentityMissingPackageRoot {
                actual: actual.as_str().to_owned(),
            }
        }
        ImportedIdentitySurfaceError::PackageRootHasContainer { container } => {
            DiagnosticPackageInterfaceFailure::IdentityPackageRootHasContainer(container.raw())
        }
        ImportedIdentitySurfaceError::PackageIdentityMismatch { symbol } => {
            DiagnosticPackageInterfaceFailure::IdentityPackageMismatch(symbol.raw())
        }
        ImportedIdentitySurfaceError::SymbolKindMismatch {
            symbol,
            declared,
            keyed,
        } => DiagnosticPackageInterfaceFailure::IdentitySymbolKindMismatch {
            record: symbol.raw(),
            declared: declared.as_str().to_owned(),
            keyed: keyed.as_str().to_owned(),
        },
        ImportedIdentitySurfaceError::DuplicateExternalKey { first, duplicate } => {
            DiagnosticPackageInterfaceFailure::IdentityDuplicateExternalKey {
                first: first.raw(),
                duplicate: duplicate.raw(),
            }
        }
        ImportedIdentitySurfaceError::MissingContainer { symbol } => {
            DiagnosticPackageInterfaceFailure::IdentityMissingContainer(symbol.raw())
        }
        ImportedIdentitySurfaceError::InvalidContainer { symbol, container } => {
            DiagnosticPackageInterfaceFailure::IdentityInvalidContainer {
                record: symbol.raw(),
                container: container.raw(),
            }
        }
        ImportedIdentitySurfaceError::ContainerKeyMismatch { symbol, container } => {
            DiagnosticPackageInterfaceFailure::IdentityContainerKeyMismatch {
                record: symbol.raw(),
                container: container.raw(),
            }
        }
        ImportedIdentitySurfaceError::UnexpectedRoot { symbol, kind } => {
            DiagnosticPackageInterfaceFailure::IdentityUnexpectedRoot {
                record: symbol.raw(),
                kind: kind.as_str().to_owned(),
            }
        }
    };

    package_failure_diagnostic(failure, product, target)
}

#[derive(Clone, Copy)]
enum RelationshipFailureKind {
    SymbolOutOfBounds,
    Invalid,
    DuplicatePosition,
}

fn relationship_failure(
    relationship: bray_package_interface::SymbolRelationship,
    kind: RelationshipFailureKind,
) -> DiagnosticPackageInterfaceFailure {
    let owner = relationship.owner().raw();
    let member = relationship.member().raw();
    let ordinal = relationship.ordinal();

    match kind {
        RelationshipFailureKind::SymbolOutOfBounds => {
            DiagnosticPackageInterfaceFailure::RelationshipSymbolOutOfBounds {
                owner,
                member,
                ordinal,
            }
        }
        RelationshipFailureKind::Invalid => {
            DiagnosticPackageInterfaceFailure::InvalidRelationship {
                owner,
                member,
                ordinal,
            }
        }
        RelationshipFailureKind::DuplicatePosition => {
            DiagnosticPackageInterfaceFailure::DuplicateRelationshipPosition {
                owner,
                member,
                ordinal,
            }
        }
    }
}

pub(super) fn query_failure_diagnostics(
    error: &FactQueryError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> DiagnosticBag {
    let failure = match error {
        FactQueryError::Cancelled => return DiagnosticBag::new(),
        FactQueryError::Cycle(_) => DiagnosticEmissionEvaluationFailure::Cycle,
        FactQueryError::InfrastructureFailure => {
            DiagnosticEmissionEvaluationFailure::Infrastructure
        }
        FactQueryError::SemanticUnitContext(_) => {
            DiagnosticEmissionEvaluationFailure::SemanticContext
        }
        FactQueryError::CheckerInfrastructure(_) => {
            DiagnosticEmissionEvaluationFailure::CheckerInfrastructure
        }
    };

    emission_failure_diagnostics(
        DiagnosticEmissionFailure::Evaluation(failure),
        product,
        target,
    )
}
